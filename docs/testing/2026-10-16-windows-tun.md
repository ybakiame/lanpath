# Windows TUN 授权修复验证

日期：2026-10-16。

Windows 管理员组 RID 曾误写为 0x22（34），构造 S-1-5-32-34；正确 Administrators SID
为 S-1-5-32-544（0x220）。改为直接使用 SDK 常量，新增 Windows 专用 IsWellKnownSid 测试，
不靠当前进程碰巧有权限判断 SID 是否正确。

UAC 成功后经 Tauri 正常退出旧实例，保留已有核心和系统代理的退出清理；取消 UAC 时保留原实例，
已具备权限时直接返回，不再重复启动实例。未改变提权方式或应用身份。

本地 Nix Rust 完整门禁、独立桌面壳 clippy、Android target check 与 Android debug APK/AAB
构建均通过。Windows 专用 SID 测试及 UAC/TUN 建卡尚未在 Windows 设备执行，需原生 Windows
构建与设备验收；未对用户正在运行的 Windows 应用执行提权、安装或重启。
