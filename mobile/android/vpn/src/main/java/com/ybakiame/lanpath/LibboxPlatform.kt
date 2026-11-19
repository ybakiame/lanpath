package com.ybakiame.lanpath

import android.content.pm.PackageManager.NameNotFoundException
import android.net.ConnectivityManager
import android.net.IpPrefix
import android.net.NetworkCapabilities
import android.net.VpnService
import android.os.ParcelFileDescriptor
import android.system.OsConstants
import android.util.Log
import com.ybakiame.lanpath.core.libbox.AutoRedirectHandler
import com.ybakiame.lanpath.core.libbox.AutoRedirectSession
import com.ybakiame.lanpath.core.libbox.BridgeOptions
import com.ybakiame.lanpath.core.libbox.BridgeSession
import com.ybakiame.lanpath.core.libbox.ConnectionOwner
import com.ybakiame.lanpath.core.libbox.InterfaceUpdateListener
import com.ybakiame.lanpath.core.libbox.Libbox
import com.ybakiame.lanpath.core.libbox.LocalDNSTransport
import com.ybakiame.lanpath.core.libbox.NetworkInterface as LibboxNetworkInterface
import com.ybakiame.lanpath.core.libbox.NetworkInterfaceIterator
import com.ybakiame.lanpath.core.libbox.NeighborUpdateListener
import com.ybakiame.lanpath.core.libbox.PlatformInterface
import com.ybakiame.lanpath.core.libbox.PlatformUser
import com.ybakiame.lanpath.core.libbox.RoutePrefix
import com.ybakiame.lanpath.core.libbox.ShellSession
import com.ybakiame.lanpath.core.libbox.StringIterator
import com.ybakiame.lanpath.core.libbox.TunOptions
import com.ybakiame.lanpath.core.libbox.WIFIState
import com.ybakiame.lanpath.core.libbox.Notification as LibboxNotification
import java.net.InetAddress
import java.net.NetworkInterface

internal class LibboxPlatform(
  private val service: VpnService,
  private val onTun: (ParcelFileDescriptor) -> Unit,
) : PlatformInterface {
  override fun usePlatformAutoDetectInterfaceControl(): Boolean = false

  override fun autoDetectInterfaceControl(fd: Int) {
    service.protect(fd)
  }

  override fun openTun(options: TunOptions): Int {
    if (VpnService.prepare(service) != null) {
      throw IllegalStateException("android: missing vpn permission")
    }

    val builder =
      service.Builder()
        .setSession(service.applicationInfo.loadLabel(service.packageManager).toString())
        .setMtu(options.mtu)

    builder.setMetered(false)

    // Exclude the core process to prevent outbound traffic looping back into the VPN.
    try {
      builder.addDisallowedApplication(service.packageName)
    } catch (e: NameNotFoundException) {
      Log.w("LibboxPlatform", "addDisallowedApplication(self) failed: ${e.message}")
    }

    val inet4Address = options.inet4Address
    while (inet4Address.hasNext()) {
      val address = inet4Address.next()
      builder.addAddress(address.address(), address.prefix())
    }

    val inet6Address = options.inet6Address
    while (inet6Address.hasNext()) {
      val address = inet6Address.next()
      builder.addAddress(address.address(), address.prefix())
    }

    if (options.autoRoute) {
      try {
        val dnsServers = options.dnsServerAddress
        while (dnsServers.hasNext()) {
          val dnsServer = dnsServers.next()
          if (!dnsServer.isNullOrEmpty()) {
            builder.addDnsServer(dnsServer)
          }
        }
      } catch (e: Exception) {
        Log.w("LibboxPlatform", "no dns server address available: ${e.message}")
      }

      val inet4RouteAddress = options.inet4RouteAddress
      if (inet4RouteAddress.hasNext()) {
        while (inet4RouteAddress.hasNext()) {
          val route = inet4RouteAddress.next()
          builder.addRoute(route.address(), route.prefix())
        }
      } else {
        builder.addRoute("0.0.0.0", 0)
      }

      val inet6RouteAddress = options.inet6RouteAddress
      if (inet6RouteAddress.hasNext()) {
        while (inet6RouteAddress.hasNext()) {
          val route = inet6RouteAddress.next()
          builder.addRoute(route.address(), route.prefix())
        }
      } else {
        builder.addRoute("::", 0)
      }

      val inet4RouteExclude = options.inet4RouteExcludeAddress
      while (inet4RouteExclude.hasNext()) {
        addExcludeRoute(builder, inet4RouteExclude.next())
      }

      val inet6RouteExclude = options.inet6RouteExcludeAddress
      while (inet6RouteExclude.hasNext()) {
        addExcludeRoute(builder, inet6RouteExclude.next())
      }

      val includePackage = options.includePackage
      while (includePackage.hasNext()) {
        try {
          builder.addAllowedApplication(includePackage.next())
        } catch (e: NameNotFoundException) {
          Log.w("LibboxPlatform", "addAllowedApplication failed: ${e.message}")
        }
      }

      val excludePackage = options.excludePackage
      while (excludePackage.hasNext()) {
        try {
          builder.addDisallowedApplication(excludePackage.next())
        } catch (e: NameNotFoundException) {
          Log.w("LibboxPlatform", "addDisallowedApplication failed: ${e.message}")
        }
      }
    }

    // Retain the original descriptor; libbox duplicates it. The service closes it on stop.
    val pfd =
      builder.establish()
        ?: throw IllegalStateException(
          "android: 无法建立 VPN 隧道（授权可能已被系统撤销）"
        )

    onTun(pfd)
    return pfd.fd
  }

  override fun useProcFS(): Boolean = false

  override fun findConnectionOwner(
    ipProtocol: Int,
    sourceAddress: String,
    sourcePort: Int,
    destinationAddress: String,
    destinationPort: Int,
  ): ConnectionOwner {
    val owner = ConnectionOwner()
    owner.userId = -1
    owner.userName = ""
    owner.processPath = ""
    return owner
  }

  override fun startDefaultInterfaceMonitor(listener: InterfaceUpdateListener) {
    DefaultInterfaceMonitor.start(listener, service.getSystemService(ConnectivityManager::class.java))
  }

  override fun closeDefaultInterfaceMonitor(listener: InterfaceUpdateListener) {
    DefaultInterfaceMonitor.close(listener)
  }

  override fun getInterfaces(): NetworkInterfaceIterator {
    val connectivity = service.getSystemService(ConnectivityManager::class.java)
    val networks = connectivity.allNetworks
    val networkInterfaces =
      runCatching { NetworkInterface.getNetworkInterfaces().toList() }.getOrDefault(emptyList())
    val interfaces = mutableListOf<LibboxNetworkInterface>()
    for (network in networks) {
      val boxInterface = LibboxNetworkInterface()
      val linkProperties = connectivity.getLinkProperties(network) ?: continue
      val networkCapabilities = connectivity.getNetworkCapabilities(network) ?: continue
      boxInterface.name = linkProperties.interfaceName
      val networkInterface = networkInterfaces.find { it.name == boxInterface.name } ?: continue
      boxInterface.dnsServer = StringArray(
        linkProperties.dnsServers.mapNotNull { it.hostAddress }.iterator()
      )
      boxInterface.type =
        when {
          networkCapabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ->
            Libbox.InterfaceTypeWIFI
          networkCapabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) ->
            Libbox.InterfaceTypeCellular
          networkCapabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) ->
            Libbox.InterfaceTypeEthernet
          else -> Libbox.InterfaceTypeOther
        }
      boxInterface.index = networkInterface.index
      runCatching { boxInterface.mtu = networkInterface.mtu }
      boxInterface.addresses = StringArray(
        networkInterface.interfaceAddresses.map { it.toPrefix() }.iterator()
      )
      var dumpFlags = 0
      if (networkCapabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)) {
        dumpFlags = OsConstants.IFF_UP or OsConstants.IFF_RUNNING
      }
      if (networkInterface.isLoopback) {
        dumpFlags = dumpFlags or OsConstants.IFF_LOOPBACK
      }
      if (networkInterface.isPointToPoint) {
        dumpFlags = dumpFlags or OsConstants.IFF_POINTOPOINT
      }
      if (networkInterface.supportsMulticast()) {
        dumpFlags = dumpFlags or OsConstants.IFF_MULTICAST
      }
      boxInterface.flags = dumpFlags
      boxInterface.metered =
        !networkCapabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
      interfaces.add(boxInterface)
    }
    return InterfaceArray(interfaces.iterator())
  }

  override fun underNetworkExtension(): Boolean = false

  override fun includeAllNetworks(): Boolean = false

  override fun readWIFIState(): WIFIState = WIFIState("", "")

  override fun clearDNSCache() {}

  override fun sendNotification(notification: LibboxNotification) {}

  override fun cancelNotification(identifier: String, typeID: Int) {}

  override fun startNeighborMonitor(listener: NeighborUpdateListener?) {}

  override fun closeNeighborMonitor(listener: NeighborUpdateListener?) {}

  override fun registerMyInterface(name: String?) {}

  override fun usePlatformShell(): Boolean = false

  override fun checkPlatformShell() {
    throw UnsupportedOperationException("android: platform shell not supported")
  }

  override fun openShellSession(
    user: PlatformUser?,
    command: String?,
    environ: StringIterator?,
    term: String?,
    rows: Int,
    cols: Int,
  ): ShellSession {
    throw UnsupportedOperationException("android: platform shell not supported")
  }

  override fun lookupUser(username: String?): PlatformUser {
    throw UnsupportedOperationException("android: platform shell not supported")
  }

  override fun lookupSFTPServer(): String {
    throw UnsupportedOperationException("android: sftp not supported")
  }

  override fun readSystemSSHHostKey(): String {
    throw UnsupportedOperationException("android: system ssh host key not supported")
  }

  override fun tailscaleHostname(): String = ""

  override fun usePlatformBridge(): Boolean = false

  override fun createBridge(options: BridgeOptions?): BridgeSession {
    throw UnsupportedOperationException("android: bridge not supported")
  }

  override fun usePlatformAutoRedirect(): Boolean = false

  override fun createAutoRedirect(
    options: ByteArray?,
    handler: AutoRedirectHandler?,
  ): AutoRedirectSession {
    throw UnsupportedOperationException("android: auto redirect not supported")
  }

  override fun localDNSTransport(): LocalDNSTransport? = null

  private fun addExcludeRoute(builder: VpnService.Builder, route: RoutePrefix) {
    try {
      builder.excludeRoute(IpPrefix(InetAddress.getByName(route.address()), route.prefix()))
    } catch (e: Exception) {
      Log.w("LibboxPlatform", "excludeRoute failed for ${route.address()}: ${e.message}")
    }
  }
}
