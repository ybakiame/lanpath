package com.ybakiame.lanpath

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Handler
import android.os.Looper
import android.util.Log
import com.ybakiame.lanpath.core.libbox.InterfaceUpdateListener
import com.ybakiame.lanpath.core.libbox.NetworkInterface as LibboxNetworkInterface
import com.ybakiame.lanpath.core.libbox.NetworkInterfaceIterator
import com.ybakiame.lanpath.core.libbox.StringIterator
import java.net.Inet6Address
import java.net.InterfaceAddress
import java.net.NetworkInterface

internal object DefaultInterfaceMonitor {
  private const val TAG = "DefaultInterfaceMonitor"
  private const val MAX_RETRY = 10
  private const val RETRY_INTERVAL_MS = 100L

  private val lock = Any()
  private val mainHandler = Handler(Looper.getMainLooper())

  private var connectivity: ConnectivityManager? = null
  private var listener: InterfaceUpdateListener? = null
  private var defaultNetwork: Network? = null
  private var registered = false

  private val request =
    NetworkRequest.Builder()
      .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
      .addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_RESTRICTED)
      .build()

  private val callback =
    object : ConnectivityManager.NetworkCallback() {
      override fun onAvailable(network: Network) {
        setNetwork(network)
      }

      override fun onCapabilitiesChanged(
        network: Network,
        networkCapabilities: NetworkCapabilities,
      ) {
        if (networkCapabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) {
          clearNetwork(network)
        } else {
          setNetwork(network)
        }
      }

      override fun onLost(network: Network) {
        clearNetwork(network)
      }
    }

  fun start(listener: InterfaceUpdateListener?, connectivity: ConnectivityManager) {
    synchronized(lock) {
      this.connectivity = connectivity
      this.listener = listener
      if (!registered) {
        register(connectivity)
        registered = true
      }

      val active = connectivity.activeNetwork
      defaultNetwork = if (active != null && isVpnNetwork(connectivity, active)) null else active
    }
    push()
  }

  fun close(listener: InterfaceUpdateListener?) {
    synchronized(lock) {
      if (this.listener !== listener) {
        return
      }
      this.listener = null
      defaultNetwork = null
      if (registered) {
        runCatching { connectivity?.unregisterNetworkCallback(callback) }
        registered = false
      }
    }
  }

  private fun setNetwork(network: Network) {
    synchronized(lock) {
      defaultNetwork = network
    }
    push()
  }

  private fun clearNetwork(network: Network) {
    synchronized(lock) {
      if (network == defaultNetwork) {
        defaultNetwork = null
      }
    }
    push()
  }

  private fun isVpnNetwork(connectivity: ConnectivityManager, network: Network): Boolean =
    connectivity.getNetworkCapabilities(network)
      ?.hasTransport(NetworkCapabilities.TRANSPORT_VPN) == true

  private fun register(connectivity: ConnectivityManager) {
    connectivity.registerBestMatchingNetworkCallback(request, callback, mainHandler)
  }

  private fun push() {
    val listener = synchronized(lock) { listener }
    val connectivity = synchronized(lock) { connectivity }
    val network = synchronized(lock) { defaultNetwork }
    if (listener == null) {
      return
    }
    if (connectivity == null) {
      return
    }

    Thread { pushWorker(listener, connectivity, network) }.start()
  }

  private fun pushWorker(
    listener: InterfaceUpdateListener?,
    connectivity: ConnectivityManager,
    network: Network?,
  ) {
    if (network == null) {
      listener?.updateDefaultInterface("", -1, false, false)
      return
    }
    for (attempt in 0 until MAX_RETRY) {
      if (synchronized(lock) { defaultNetwork } !== network) {
        return
      }
      val linkProperties = connectivity.getLinkProperties(network)
      val name = linkProperties?.interfaceName
      val index =
        name?.let { runCatching { NetworkInterface.getByName(it).index }.getOrNull() }
      if (name.isNullOrEmpty() || index == null) {
        Thread.sleep(RETRY_INTERVAL_MS)
        continue
      }

      listener?.updateDefaultInterface(name, index, false, false)
      return
    }

    Log.w(TAG, "failed to resolve default interface after $MAX_RETRY attempts")
  }
}

internal class StringArray(private val iterator: Iterator<String>) : StringIterator {
  override fun len(): Int = 0
  override fun hasNext(): Boolean = iterator.hasNext()
  override fun next(): String = iterator.next()
}

internal class InterfaceArray(
  private val iterator: Iterator<LibboxNetworkInterface>,
) : NetworkInterfaceIterator {
  override fun hasNext(): Boolean = iterator.hasNext()
  override fun next(): LibboxNetworkInterface = iterator.next()
}

internal fun InterfaceAddress.toPrefix(): String =
  if (address is Inet6Address) {
    "${Inet6Address.getByAddress(address.address).hostAddress}/$networkPrefixLength"
  } else {
    "${address.hostAddress}/$networkPrefixLength"
  }
