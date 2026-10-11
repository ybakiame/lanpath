# ADR-0002 桌面产品数据目录与便携分发

状态：Accepted · 日期：2026-10-11

## 背景

用户要求增加 Portable 版，并让安装版数据目录以产品名 `.lanpath` 表达。
业务数据已使用产品目录，但 Windows/Linux WebView 默认路径由 Tauri identifier 决定。

## 决定

- 保持应用身份 `com.ybakiame.lanpath`，在壳层显式配置桌面数据布局。
- 可执行文件旁存在 `lanpath.portable` 普通文件时，使用绝对路径 `<exe目录>/data`；否则使用用户主目录 `.lanpath`，保留既有 `.proxy-panel-client` 位置兼容。解析失败不使用工作目录兜底。
- Windows/Linux 在 setup 中创建桌面窗口前设置 WebView 的数据目录为 `<数据根>/webview`。Android 保持自动建窗和应用私有数据路径。macOS 不支持目录型 WebView 数据隔离，不提供 Portable 包。
- Windows x64/arm64 的同一二进制提供 NSIS 与 Portable ZIP。ZIP 包含程序、标记、空 data、锁定种子资源和使用说明；依赖系统 WebView2 Runtime。
- Portable 不注册安装器 updater，更新入口打开发布页，用户退出后手动替换程序，保留 data。安装版签名更新清单不变。
- 数据根改变时仅在内存中重定位核心托管路径、配置根和证书根，外部核心路径保持原值。不复制、删除或自动改写旧用户数据。

## 后果

便携数据不随工作目录改变，可整目录移动；安装版、便携版隔离。
原 identifier WebView 目录不自动迁移，既有主题偏好需重新设置。任何未来自动迁移需单独授权并设计备份与回退。
Windows 原生解压运行、WebView2、TUN 和真实升级验收仍需要 Windows 设备；Linux 主机检查不能替代。

实现依据：[Tauri WebviewWindowBuilder](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html#method.data_directory)；WKWebView 的存储标识不能替代可移动的目录。
