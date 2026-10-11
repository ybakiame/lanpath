# Lanpath 架构

apps/client/src/pages 与 @lanpath/ui 共用双端页面。@lanpath/client-core 是命令、状态与 hooks 入口。
apps/client/src-tauri 为独立 Cargo 壳，经 target 依赖门控区分桌面/Android。
lanpath-tauri 实现共享命令；lanpath-client 管理订阅、配置、策略、流量与本地 SQLite。
lanpath-engine 只保留 sing-box 管理器和进程生命周期；lanpath-common 仅含错误、CoreType 和 OS 工具，
不引入 Sea-ORM、tonic 或服务端模型。lanpath-mitm/lanpath-script 属于客户端。
Android 经 Kotlin VpnPlugin 和 com.ybakiame.lanpath.core.libbox 运行内嵌 Go 引擎。

客户端不依赖 Hub/Agent/Proto/DB crate，订阅与可选服务端集成通过公开协议。
保留既有核心父子绑定、PID 校验、退出清理和端口检查。系统调用不得阻塞窗口线程。
桌面窗口、MITM 和更新属于平台能力，Android APK 不含桌面 updater/MITM 构建链。

主题偏好与实际深浅解析由 @lanpath/client-core 管理；深色纯黑背景是附属布尔偏好，
不增加主题模式或核心配置字段。应用 ThemeProvider 管理强调色并将编译期平台投影为 data-platform；
共享 UI 消费语义令牌。主题二级页为 /settings/theme，移动与桌面共用路由和控件。
