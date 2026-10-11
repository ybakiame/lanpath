# 运维、迁移与发布

新 Android 包名 com.ybakiame.lanpath 是独立应用，无法覆盖旧 com.proxypanel.client。
保留旧包；导入/重新配置后再验收。当前不提供跨包私有数据自动迁移，VPN/通知等权限需重新授权。
桌面默认 ~/.lanpath；新目录不存在而 ~/.proxy-panel-client 存在时沿用旧目录，不删除或复制旧数据。
Windows/Linux 的 WebView 本地存储与缓存放在所选数据根的 `webview/`；不再由 identifier 决定路径。
原 identifier WebView 目录保留且不自动导入，因此升级后原 WebView 中的外观偏好需重新设置；业务订阅和配置仍使用原数据根。
macOS WKWebView 不支持此目录 API，当前没有 macOS Portable 分发包，不能宣称其浏览器数据可便携。
不要同时运行新旧客户端共享同一个数据目录。旧 MITM CA 名称和系统信任关系保留，避免更名造成重建。
已有配置键和数据库 schema 保留，任何后续迁移使用版本化逻辑。

公开仓库为 ybakiame/lanpath，默认 main；首次发版为 v0.1.1-beta.1。
首次发版经用户明确授权生成 Lanpath 独立 updater 密钥与 Android release 证书；
旧产品公钥不再用于新独立客户端。私钥保存在本机安全目录及新仓 Secrets，均不入库。
本机备份目录为 `~/.local/share/lanpath/signing`（目录权限 700，私钥/凭据权限 600）。
不能从 GitHub Secrets 下载原值；后续版本必须保留本次密钥，备份完整目录。
首次身份更改无法作为旧 Android 包的原地升级；Windows 安装器更名需真实升级验收。

Windows Build（`.github/workflows/windows-build.yaml`）支持手动测试与发布调用，构建 NSIS 和 Portable，仅上传 artifact；发布工作流通过手动输入匹配版本 tag，
构建包、生成签名更新清单后创建 Release。不同时更换打包方案。
签名构建额外使用 `tauri.updater.conf.json` 的 v1Compatible overlay，产生 `.nsis.zip` 与 `.sig`；
未签名测试构建不启用该 overlay。Portable 与安装版从同一份二进制组装。
发布任务只接受相同 commit 已成功的 Client CI；发布前以代码中的公钥实际验证两种架构的更新签名。
Android 独立工作流经 Nix 从 Go 源码构建 AAR；CI 生成 debug APK，发版使用专用 release keystore 签名并经 apksigner 验证。
release Secrets：TAURI_SIGNING_PRIVATE_KEY、TAURI_SIGNING_PRIVATE_KEY_PASSWORD、ANDROID_KEY_BASE64、ANDROID_KEY_PASSWORD、ANDROID_KEY_ALIAS。
keystore.properties 与 CI 临时 keystore 均不提交，任务结束清理；本地仍可构建未签名 release，再显式签名。
测试版仅输出 `prerelease.json`，稳定版输出 `latest.json`；beta/rc 应用与 Portable 的检查更新打开发布列表，避免混入稳定自动更新通道。
发布文件附带 SHA256SUMS；Windows 更新签名不等同 Authenticode，当前未配置 Windows 代码签名证书。
协议子模块为私有治理资源，编译不依赖它；公开 CI 不递归检出子模块，也不向 fork PR 提供私有 token。
GitHub Linux runner 清理预装 SDK 腾出空间，然后使用项目 Nix SDK；云端明确使用 cache.nixos.org，避免继承本机镜像配置。
Windows CI 例外使用锁定 Rust/Bun 和固定 Windows runner 标签；本地、Linux/Android 构建仍 Nix。

## Windows Portable

测试构建与正式发布同时输出 NSIS 安装器和 `Lanpath_<版本>_windows_<x64|arm64>-portable.zip`。
Portable ZIP 由同一桌面二进制和锁定种子核心组装，不含任何开发或用户数据。
完整解压到可写目录，运行 `Lanpath.exe`。系统须已安装 Microsoft Edge WebView2 Runtime；ZIP 不安装运行时、创建快捷方式或执行安装器。

程序旁的 `lanpath.portable` 文件决定便携模式，数据根固定为该可执行文件所在目录的 `data/`，不使用终端或快捷方式的工作目录。
配置、订阅、规则、证书、核心、统计、日志和 WebView 偏好均基于该数据根。标记无效、目录解析或创建失败时终止启动，不回退到用户主目录。
安装版与便携版互不读取数据；旧产品目录兼容仅在安装模式生效。

便携版不注册安装器 updater；「检查更新」打开发布页并提示手动更新。
更新前退出程序，保留 `data/`，用新版 ZIP 中的程序和 `seed/` 替换旧文件，保留 `lanpath.portable`。
移动整个便携目录时，托管于 `data/cores` 的核心和证书路径在内存中重定位；手动指定的外部核心路径需用户自行调整。
不自动迁移、复制或删除已有安装版/旧 WebView 目录。不要同时运行两个实例共享同一份数据。

Windows CI 在构建 NSIS 后调用 `bun apps/client/scripts/package-portable.ts <目标三元组>` 组装 ZIP；发布流程下载同一 artifact 前缀并发布两种包，`latest.json` 继续只指向签名的安装版更新包。
MSVC 14.44.35207 与 SDK 10.0.26100.0 显式选取，缺失时构建失败，不自动回退；由 runner 提供并记录版本，升级需单独复查；不声称托管镜像是不可变环境。
