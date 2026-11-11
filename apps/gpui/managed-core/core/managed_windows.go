package main

import tun "github.com/sagernet/sing-tun"

func init() {
	managedCoreReadyHook = tun.ManagedCoreReady
}
