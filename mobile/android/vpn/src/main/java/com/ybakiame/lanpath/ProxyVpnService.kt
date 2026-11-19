package com.ybakiame.lanpath

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.VpnService
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.ybakiame.lanpath.core.libbox.CommandServer
import com.ybakiame.lanpath.core.libbox.CommandServerHandler
import com.ybakiame.lanpath.core.libbox.Libbox
import com.ybakiame.lanpath.core.libbox.OverrideOptions
import com.ybakiame.lanpath.core.libbox.SetupOptions
import com.ybakiame.lanpath.core.libbox.SystemProxyStatus

/** Foreground VPN service. Core and TUN shutdown remain serialized across instances. */
class ProxyVpnService : VpnService(), CommandServerHandler {
  companion object {
    private const val TAG = "ProxyVpnService"
    private const val CHANNEL_ID = "vpn"
    private const val NOTIFICATION_ID = 1

    const val EXTRA_CONFIG = "config"

    const val EXTRA_SHOW_SELECTION = "show_selection"

    const val EXTRA_SUBSCRIPTION_NAME = "subscription_name"

    const val ACTION_STOP = "com.ybakiame.lanpath.action.STOP"

    const val ACTION_UPDATE_PREFS = "com.ybakiame.lanpath.action.UPDATE_PREFS"

    @Volatile
    var running = false
      private set

    @Volatile
    var instanceAlive = false
      private set

    @Volatile
    var lastError: String? = null
      private set

    private val setupLock = Any()

    @Volatile
    private var setupDone = false

    // An older service instance must never close the current instance's core or TUN.
    private val lifecycleLock = Any()

    @Volatile
    private var startInProgress = false

    @Volatile
    private var commandServer: CommandServer? = null

    private var tunFd: ParcelFileDescriptor? = null

    private var boxOwner: ProxyVpnService? = null
  }

  @Volatile
  private var stopRequested = false

  @Volatile
  private var lastConfig: String? = null

  @Volatile
  private var showSelection = true

  @Volatile
  private var subscriptionName: String = ""

  private val notifications = VpnNotification(this, { showSelection }, { subscriptionName })

  private val logs = LibboxLogs(this)
  private val mainHandler = Handler(Looper.getMainLooper())

  private val platformInterface = LibboxPlatform(this) { tunFd = it }

  override fun onCreate() {
    super.onCreate()
    instanceAlive = true
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      handleStop()
      return Service.START_NOT_STICKY
    }

    if (intent?.action == ACTION_UPDATE_PREFS) {
      showSelection = intent.getBooleanExtra(EXTRA_SHOW_SELECTION, showSelection)
      subscriptionName = intent.getStringExtra(EXTRA_SUBSCRIPTION_NAME) ?: subscriptionName
      if (running) {
        notifications.updateNotificationContent()
      }
      return Service.START_NOT_STICKY
    }

    val config = intent?.getStringExtra(EXTRA_CONFIG)
    if (config.isNullOrBlank()) {
      Log.w(TAG, "start command without config, stopping")
      lastError = "Missing VPN config"
      stopSelf()
      return Service.START_NOT_STICKY
    }

    showSelection = intent.getBooleanExtra(EXTRA_SHOW_SELECTION, VpnController.showSelection)
    subscriptionName = intent.getStringExtra(EXTRA_SUBSCRIPTION_NAME)
      ?: (VpnController.subscriptionName ?: "")

    try {
      startForegroundWithNotification()
    } catch (e: Exception) {
      Log.e(TAG, "failed to start foreground service", e)
      running = false
      lastError = "Foreground service start failed: ${e.message ?: e.javaClass.simpleName}"
      stopSelf()
      return Service.START_NOT_STICKY
    }

    if (startInProgress) {
      Log.w(TAG, "start ignored: another start sequence is in progress")
      return Service.START_NOT_STICKY
    }

    stopRequested = false

    notifications.parseClashApiConfig(config)

    startInProgress = true
    Thread {
      try {
        startBox(config)
        if (stopRequested) {
          Log.i(TAG, "stop requested during startup, closing service")
          stopBox()
        } else {
          running = true
          lastError = null
          Log.i(TAG, "libbox service started")

          notifications.startNotificationPolling()
        }
      } catch (e: Exception) {
        Log.e(TAG, "failed to start libbox service", e)
        running = false

        lastError = logs.withTail(buildExceptionChain(e))
        stopSelf()
      } finally {
        startInProgress = false
      }
    }.start()

    return Service.START_NOT_STICKY
  }

  private fun handleStop() {
    stopRequested = true
    running = false
    notifications.stopNotificationPolling()
    Log.i(TAG, "stop requested")
    Thread {
      try {
        stopBox()
      } catch (e: Exception) {
        Log.e(TAG, "failed to stop libbox service", e)
        lastError = buildExceptionChain(e)
      } finally {
        mainHandler.post {
          stopForegroundCompat()
          stopSelf()
        }
      }
    }.start()
  }

  override fun onDestroy() {
    instanceAlive = false

    closeBoxInBackground()
    super.onDestroy()
  }

  override fun onRevoke() {
    stopRequested = true
    closeBoxInBackground()
    stopSelf()
    super.onRevoke()
  }

  private fun startForegroundWithNotification() {
    val channel =
      NotificationChannel(CHANNEL_ID, "岚径 VPN", NotificationManager.IMPORTANCE_LOW)
    getSystemService(NotificationManager::class.java).createNotificationChannel(channel)

    val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
    val contentIntent =
      launchIntent?.let {
        PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_IMMUTABLE)
      }

    val notification =
      NotificationCompat.Builder(this, CHANNEL_ID)
        .setContentTitle("岚径 · Lanpath")
        .setContentText("VPN service is running")
        .setSmallIcon(R.drawable.ic_stat_lanpath)
        .setContentIntent(contentIntent)
        .setOngoing(true)
        .build()

    val type =
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
      } else {
        0
      }
    ServiceCompat.startForeground(this, NOTIFICATION_ID, notification, type)
  }

  private fun startBox(config: String) {
    val workingDir = getExternalFilesDir(null)
    val setup =
      SetupOptions().apply {
        basePath = filesDir.absolutePath
        workingPath = workingDir?.absolutePath ?: filesDir.absolutePath
        tempPath = cacheDir.absolutePath
        fixAndroidStack = true
        logMaxLines = LibboxLogs.BUFFER_SIZE.toLong()
      }
    setupLibbox(setup)

    lastConfig = config

    synchronized(lifecycleLock) {
      logs.stop()
      commandServer?.let { old ->
        commandServer = null
        boxOwner = null
        try {
          old.closeService()
        } catch (e: Exception) {
          Log.e(TAG, "failed to close previous libbox service", e)
        }
        try {
          old.close()
        } catch (e: Exception) {
          Log.e(TAG, "failed to close previous command server", e)
        }
      }
      tunFd?.let { old ->
        try {
          old.close()
        } catch (e: Exception) {
          Log.e(TAG, "failed to close previous tun fd", e)
        }
      }
      tunFd = null

      val server = CommandServer(this, platformInterface)
      try {
        server.start()
        commandServer = server

        logs.start()
        server.startOrReloadService(config, OverrideOptions())
      } catch (e: Exception) {
        logs.stop()
        runCatching { server.closeService() }
        runCatching { server.close() }
        runCatching { tunFd?.close() }
        tunFd = null
        if (commandServer === server) {
          commandServer = null
        }
        throw e
      }
      boxOwner = this
    }
  }

  override fun serviceStop() {
    Log.i(TAG, "libbox requested service stop")
    handleStop()
  }

  override fun serviceReload() {
    val config = lastConfig ?: return
    Log.i(TAG, "libbox requested service reload")
    Thread {
      try {
        synchronized(lifecycleLock) {
          val server = commandServer ?: return@synchronized
          runCatching { tunFd?.close() }
          tunFd = null
          server.startOrReloadService(config, OverrideOptions())
        }
      } catch (e: Exception) {
        Log.e(TAG, "libbox service reload failed", e)
        lastError = buildExceptionChain(e)
      }
    }.start()
  }

  override fun getSystemProxyStatus(): SystemProxyStatus {
    val status = SystemProxyStatus()
    status.available = false
    status.enabled = false
    return status
  }

  override fun setSystemProxyEnabled(isEnabled: Boolean) {}

  override fun triggerNativeCrash() {
    Thread {
      Thread.sleep(200)
      throw RuntimeException("debug native crash")
    }.start()
  }

  override fun writeDebugMessage(message: String?) {
    if (message != null) {
      Log.d(TAG, message)
    }
  }

  override fun connectSSHAgent(): Int = -1

  private fun setupLibbox(setup: SetupOptions) {
    if (setupDone) {
      return
    }
    synchronized(setupLock) {
      if (setupDone) {
        return
      }
      Libbox.setup(setup)
      setupDone = true
    }
  }

  private fun closeBoxInBackground() {
    Thread {
      try {
        stopBox()
      } catch (e: Exception) {
        Log.e(TAG, "failed to stop libbox service", e)
        lastError = buildExceptionChain(e)
      }
    }.start()
  }

  private fun stopBox() {
    synchronized(lifecycleLock) {
      if (boxOwner != null && boxOwner !== this) {
        Log.i(TAG, "stop ignored: box owned by another instance")
        return
      }
      running = false
      val server = commandServer
      commandServer = null
      boxOwner = null
      logs.stop()

      try {
        tunFd?.close()
      } catch (e: Exception) {
        Log.e(TAG, "failed to close tun fd", e)
        lastError = buildExceptionChain(e)
      }
      tunFd = null
      if (server != null) {
        try {
          server.closeService()
        } catch (e: Exception) {
          Log.e(TAG, "failed to close libbox service", e)
          lastError = buildExceptionChain(e)
        }
        try {
          server.close()
        } catch (e: Exception) {
          Log.e(TAG, "failed to close command server", e)
        }
      }
    }
  }

  private fun buildExceptionChain(e: Throwable): String {
    val parts = mutableListOf<String>()
    var current: Throwable? = e
    var depth = 0
    while (current != null && depth < 10) {
      val message = current.message ?: current.javaClass.simpleName
      if (depth == 0) {
        parts.add(message)
      } else {
        parts.add("caused by: $message")
      }
      current = current.cause
      depth++
    }
    return parts.joinToString("\n")
  }

  private fun stopForegroundCompat() {
    try {
      ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    } catch (e: Exception) {
      Log.w(TAG, "stopForeground failed: ${e.message}")
    }
  }
}
