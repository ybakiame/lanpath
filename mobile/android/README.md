# Android 原生能力

独立 Gradle 库工程，使用项目 wrapper；SDK 36、最低 Android 13、arm64-v8a。

`vpn` 模块提供 `com.ybakiame.lanpath.VpnController`、`ProxyVpnService` 和 `MobileFiles`，不依赖前端运行时。宿主调用 `VpnController.prepare(context)`，自行展示返回的授权 Intent；获得授权后才调用 `start`。宿主还需处理通知授权。

`status()` 返回实际运行状态和最近错误，`stop()` 请求有序关闭，`updateNotification()` 更新通知偏好。`MobileFiles` 将日志、原始订阅和加密备份导出到公共 Downloads/Lanpath。

先构建 Go 绑定，再运行 Gradle：

```bash
devenv shell -- lanpath-build-aar
devenv shell -- lanpath-build-android-debug
```

本模块以 `compileOnly` 引用 `vpn/libs/lanpathcore.aar`。集成时，宿主必须同时依赖本模块和 Go AAR，确保 gomobile 类与 arm64 原生库实际打包；本模块不会将另一个 AAR 嵌套进自己的产物。

库清单包含 VPN 服务和权限声明，不包含启动 Activity、应用包名、签名或页面。尚未接入后续移动端宿主，不能将库构建通过视为 VPN 真机验证完成。
