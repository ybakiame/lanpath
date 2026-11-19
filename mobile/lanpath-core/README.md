# lanpath-core

固定 sing-box `v1.15.0-alpha.5` 与 SagerNet gomobile `v0.1.12`，导出 libbox Android arm64 绑定。Go module 与校验和随工程保留，不在本轮升级。

从仓库根运行 `devenv shell -- lanpath-build-aar`，或在 devenv shell 内执行 `bash mobile/lanpath-core/build-android.sh [OUTPUT_AAR]`。

默认输出 `mobile/android/vpn/libs/lanpathcore.aar`，Java 包前缀为 `com.ybakiame.lanpath.core`，最低 Android API 33。构建标签保留 `with_gvisor`，支持已有 mixed/gvisor 栈配置。

绑定包含 GPL-3.0 组件；分发时保留相应许可和源码可得性要求。AAR 不提交到仓库。
