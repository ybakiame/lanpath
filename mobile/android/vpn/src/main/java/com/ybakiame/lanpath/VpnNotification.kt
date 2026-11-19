package com.ybakiame.lanpath

import android.app.NotificationManager
import android.app.PendingIntent
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.app.NotificationCompat

internal class VpnNotification(
  private val service: ProxyVpnService,
  private val showSelection: () -> Boolean,
  private val subscriptionName: () -> String,
) {
  private var clashApiPort: Int = 9090
  private var clashApiSecret: String = ""
  private var notificationUpdateHandler: Handler? = null
  private var notificationUpdateRunnable: Runnable? = null

  fun parseClashApiConfig(config: String) {
    try {
      val json = org.json.JSONObject(config)

      val experimental = json.optJSONObject("experimental")
      if (experimental != null) {
        val clashApi = experimental.optJSONObject("clash_api")
        if (clashApi != null) {
          clashApiPort = clashApi.optInt("external_controller_port", 9090)
          clashApiSecret = clashApi.optString("secret", "")
          return
        }
      }

      val directClash = json.optJSONObject("clash_api")
      if (directClash != null) {
        clashApiPort = directClash.optInt("external_controller_port", 9090)
        clashApiSecret = directClash.optString("secret", "")
      }
    } catch (e: Exception) {
      Log.w("VpnNotification", "failed to parse clash api config: ${e.message}")
    }
  }

  fun startNotificationPolling() {
    stopNotificationPolling()
    if (!showSelection()) {
      return
    }
    notificationUpdateHandler = Handler(Looper.getMainLooper())
    val runnable = object : Runnable {
      override fun run() {
        if (!ProxyVpnService.running) return
        updateNotificationContent()
        notificationUpdateHandler?.postDelayed(this, 4000)
      }
    }
    notificationUpdateRunnable = runnable
    notificationUpdateHandler?.postDelayed(runnable, 4000)
  }

  fun stopNotificationPolling() {
    notificationUpdateRunnable?.let { notificationUpdateHandler?.removeCallbacks(it) }
    notificationUpdateRunnable = null
    notificationUpdateHandler = null
  }

  fun updateNotificationContent() {
    Thread { refreshNotification() }.start()
  }

  private fun refreshNotification() {
    try {
      if (!showSelection()) {
        updateNotification("VPN service is running")
        return
      }
      val selection = fetchClashSelection()
      val parts = mutableListOf<String>()
      if (subscriptionName().isNotEmpty()) {
        parts.add(subscriptionName())
      }
      if (selection.isNotEmpty()) {
        parts.add(selection)
      }
      val contentText = if (parts.isEmpty()) "VPN service is running" else parts.joinToString(" · ")
      updateNotification(contentText)
    } catch (e: Exception) {
      Log.w("VpnNotification", "notification update failed: ${e.message}")
    }
  }

  private fun fetchClashSelection(): String {
    return try {
      val url = java.net.URL("http://127.0.0.1:$clashApiPort/proxies")
      val conn = url.openConnection() as java.net.HttpURLConnection
      conn.connectTimeout = 2000
      conn.readTimeout = 2000
      if (clashApiSecret.isNotEmpty()) {
        conn.setRequestProperty("Authorization", "Bearer $clashApiSecret")
      }
      val text = conn.inputStream.bufferedReader().use { it.readText() }
      conn.disconnect()

      val json = org.json.JSONObject(text)
      val proxies = json.optJSONObject("proxies") ?: return ""

      val keys = proxies.keys()
      while (keys.hasNext()) {
        val key = keys.next()
        val proxy = proxies.optJSONObject(key) ?: continue
        if (proxy.optString("type") == "Selector") {
          val now = proxy.optString("now", "")
          if (now.isNotEmpty()) {
            return "$key: $now"
          }
        }
      }
      ""
    } catch (e: Exception) {
      ""
    }
  }

  private fun updateNotification(contentText: String) {
    val launchIntent = service.packageManager.getLaunchIntentForPackage(service.packageName)
    val contentIntent =
      launchIntent?.let {
        PendingIntent.getActivity(service, 0, it, PendingIntent.FLAG_IMMUTABLE)
      }
    val notification =
      NotificationCompat.Builder(service, "vpn")
        .setContentTitle("岚径 · Lanpath")
        .setContentText(contentText)
        .setSmallIcon(R.drawable.ic_stat_lanpath)
        .setContentIntent(contentIntent)
        .setOngoing(true)
        .build()
    val nm = service.getSystemService(NotificationManager::class.java)
    nm.notify(1, notification)
  }
}
