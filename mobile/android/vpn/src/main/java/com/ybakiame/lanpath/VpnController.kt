package com.ybakiame.lanpath

import android.content.Context
import android.content.Intent
import android.net.VpnService
import android.util.Log
import androidx.core.content.ContextCompat
import com.ybakiame.lanpath.core.libbox.Libbox

/** Native entry points. The host owns permission prompts and calls start after approval. */
object VpnController {
  @Volatile internal var showSelection = true
  @Volatile internal var subscriptionName: String? = null

  data class Status(val running: Boolean, val lastError: String?)

  fun prepare(context: Context): Intent? = VpnService.prepare(context)

  fun status(): Status = Status(ProxyVpnService.running, ProxyVpnService.lastError)

  fun coreVersion(): String = Libbox.version()

  fun start(context: Context, config: String, showSelection: Boolean = true, subscriptionName: String? = null) {
    require(config.isNotBlank()) { "vpn_missing_config" }
    check(prepare(context) == null) { "vpn_not_authorized" }
    this.showSelection = showSelection
    this.subscriptionName = subscriptionName
    val intent = Intent(context, ProxyVpnService::class.java)
      .putExtra(ProxyVpnService.EXTRA_CONFIG, config)
      .putExtra(ProxyVpnService.EXTRA_SHOW_SELECTION, showSelection)
      .putExtra(ProxyVpnService.EXTRA_SUBSCRIPTION_NAME, subscriptionName)
    ContextCompat.startForegroundService(context, intent)
  }

  fun stop(context: Context) {
    val intent = Intent(context, ProxyVpnService::class.java).setAction(ProxyVpnService.ACTION_STOP)
    if (ProxyVpnService.instanceAlive) {
      try {
        ContextCompat.startForegroundService(context, intent)
      } catch (e: Exception) {
        Log.w("VpnController", "stop intent failed; falling back to stopService", e)
        context.stopService(intent)
      }
    } else {
      context.stopService(intent)
    }
  }

  fun updateNotification(context: Context, showSelection: Boolean, subscriptionName: String?) {
    this.showSelection = showSelection
    this.subscriptionName = subscriptionName
    if (ProxyVpnService.instanceAlive) {
      ContextCompat.startForegroundService(context,
        Intent(context, ProxyVpnService::class.java)
          .setAction(ProxyVpnService.ACTION_UPDATE_PREFS)
          .putExtra(ProxyVpnService.EXTRA_SHOW_SELECTION, showSelection)
          .putExtra(ProxyVpnService.EXTRA_SUBSCRIPTION_NAME, subscriptionName))
    }
  }
}
