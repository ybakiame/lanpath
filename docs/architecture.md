# Lanpath 架构

## 当前发行实现

apps/client/src/pages 与 @lanpath/ui 共用双端页面。@lanpath/client-core 是命令、状态与 hooks 入口。
apps/client/src-tauri 为独立 Cargo 壳，经 target 依赖门控区分桌面/Android。
lanpath-tauri 实现共享命令；lanpath-client 管理订阅、配置、策略、流量与本地 SQLite。
lanpath-engine 只保留 sing-box 管理器和进程生命周期；lanpath-common 仅含错误、CoreType 和 OS 工具，
不引入 Sea-ORM、tonic 或服务端模型。lanpath-mitm/lanpath-script 属于客户端。
Android 经 Kotlin VpnPlugin 和 com.ybakiame.lanpath.core.libbox 运行内嵌 Go 引擎。

客户端不依赖 Hub/Agent/Proto/DB crate，订阅与可选服务端集成通过公开协议。
保留既有核心父子绑定、PID 校验、退出清理和端口检查。系统调用不得阻塞窗口线程。
桌面窗口、MITM 和更新属于平台能力，Android APK 不含桌面 updater/MITM 构建链。

桌面数据布局由壳层 DesktopStorage 统一解析：安装模式使用 `~/.lanpath`（保留旧目录位置兼容），
程序旁 `lanpath.portable` 标记启用 `<exe目录>/data`；Windows/Linux WebView 使用数据根的 `webview/`。
Windows 的同一二进制分发为 NSIS 与 Portable ZIP，便携模式禁用安装器 updater，手动保留 data 更新。
数据目录移动只在内存中重定位托管核心和证书路径，不自动迁移旧用户目录。见 [ADR-0002](adr/0002-desktop-portable-storage.md)。

首次公开 beta 使用独立 updater 公钥与 Android release 证书，CI 不检出私有协议子模块。
Windows native 构建例外保持锁定版本，Linux/Android 与发布验签使用项目 Nix；beta/rc 手动更新，稳定版使用签名 latest.json。
见 [ADR-0003](adr/0003-first-public-release.md)。

开发环境通过 standalone devenv CLI 与 devenv.nix 模块组织；devenv.yaml 声明输入，devenv.lock 是唯一项目环境锁文件，不再维护项目 Flake。CLI 固定为 2.4.0，项目工具链保持 26.05 原快照，Bun 单独固定并与 Windows 对齐；Android SDK 与 CLI/辅助程序自身的上游构建快照不借迁移升级。环境初始化不自动安装项目依赖、构建业务产物、启动代理或发布，CI/CD 复用 nix/commands.nix 中的显式命令；Husky 仍是唯一提交检查入口。见 [ADR-0015](adr/0015-standalone-devenv.md)，其取代 ADR-0004/0005 的入口与锁定条款。

主题偏好与实际深浅解析由 @lanpath/client-core 管理；深色纯黑背景是附属布尔偏好，
不增加主题模式或核心配置字段。应用 ThemeProvider 管理强调色并将编译期平台投影为 data-platform；
共享 UI 消费语义令牌。主题二级页为 /settings/theme，移动与桌面共用路由和控件。

订阅原文与内部转换缓存分离：成功拉取保存 `<id>.source.json`，查看/导出不重新请求；
YAML/JSON/分享链接分别导出 yaml/json/txt，桌面与 Android 均写入下载目录的 Lanpath 文件夹。
仪表盘管理当前订阅的覆写绑定，模板管理双端共用，既有 profile_id 语义不变。见 ADR-0006。

客户端订阅、远程资源与核心下载共用 request_network 的四种请求代理策略；GitHub 镜像仅改写 URL。
配置与 IPC 统一使用 request_proxy_mode/request_proxy_url，旧布尔开关备份后一次性迁移，见 ADR-0007。

备份由 lanpath-client::backup 负责白名单快照、age 加密与启动恢复事务；共享 BackupService
提供密文导出、一次性预览、历史与恢复结果编排。lanpath-tauri 提供平台/IPC 适配，现行 UI 经
client-core 调用；GPUI 直接复用该服务，只捕获和恢复租用的实验根。恢复在平台状态/存储初始化前
应用，Android 仅适配密文文件导出到 MediaStore，配置不进入 Go 桥。见 ADR-0008、ADR-0013。

备份 v2 额外迁移 rulesets 文件并兼容 v1；启动阶段由独立内存状态提供，
client-core 以 Query 轮询对接双端反馈，详见 ADR-0010。

规则集用户分类随 LocalOverride 持久化，分类命令与运行分流无关；路由/DNS 复用筛选多选器。
远程更新元数据由后端持有，HTTP 校验器绑定有效地址和缓存指纹；写回只合并检查结果，见 ADR-0011。

## Windows GPUI 实验性并行目标

用户已批准 [ADR-0013](adr/0013-gpui-experimental-parallel-migration.md) 的实验性完整业务迁移开发。
`apps/desktop` 是独立 Cargo 应用，不加入 Android 目标或替换 `apps/client`。
Tauri 与 GPUI 共享适用的 Rust 业务能力，各自持有 UI 状态及平台适配；React 状态规范仍只约束现行 React 页面。

实验应用使用独立身份、持久数据根、核心进程和 loopback 端口，不读取/迁移生产数据，不设置系统代理或启用 TUN。
完整功能接入与正式替换是不同决策；数据兼容、系统集成、目标架构、安装更新、回退和资源收益仍需实际验收。
根 workspace 不包含该应用，独立检查及原生 Windows 验收不由现行 Tauri 构建结果替代。
已有技术验证证据保留在 [ADR-0012](adr/0012-gpui-desktop-local-validation.md)，不将其切片覆盖当作完整客户端对等证明。
