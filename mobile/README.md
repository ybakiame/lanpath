# Lanpath 移动端基础

- `lanpath-core/`：固定版本的 sing-box/libbox Go 绑定与 Android arm64 构建入口。
- `android/`：独立 Android 原生库工程，保留 VPN 服务、网络接口回调、日志、通知和文件导出能力。

这里暂不提供移动端页面或可安装 APK。后续移动端宿主需接入原生库、共享 Rust 业务和界面，负责 VPN/通知授权及生命周期；当前 desktop 没有接入此 VPN。

从仓库根目录执行：

```bash
devenv shell -- lanpath-build-aar
devenv shell -- lanpath-build-android-debug
```

Go 绑定输出到 `android/vpn/libs/lanpathcore.aar`。Android 库输出到 `android/vpn/build/outputs/aar/`；两个 AAR 均为本地生成产物，不入库。
