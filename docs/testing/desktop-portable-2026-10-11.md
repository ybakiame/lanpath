# Desktop Portable 验证（2026-10-11）

## 范围与设计

Windows x64/arm64 使用同一桌面二进制分别组装 NSIS 与 Portable ZIP。
便携模式由程序旁 `lanpath.portable` 文件识别，使用 `<exe目录>/data`；安装模式使用用户主目录 `.lanpath`，保留旧业务目录位置兼容。
Windows/Linux WebView 使用所选数据根的 `webview`，Android 的私有目录与自动建窗保持原流程。

## 本地自动验证

所有命令经本仓 `nix develop --offline` 执行：

- `cargo build --workspace` 通过。
- `bun run verify:rust` 通过：根 fmt、clippy、workspace tests；独立桌面壳 host clippy；aarch64-linux-android check。
- 独立壳 `cargo test --manifest-path apps/client/src-tauri/Cargo.toml desktop::state::tests`：3 个路径回归通过，覆盖产品目录、旧目录兼容、标记优先、无 HOME、可执行文件目录、无效标记与不回退工作目录。
- workspace 配置测试验证数据根移动后的核心/证书重定位、外部核心保留及原配置文件不被自动改写。
- `bun test apps/client/scripts/package-portable.test.ts`：用临时假 exe/seed 组装并解压真实 ZIP，检查 x64/arm64 名称、标记、空 data、核心/DLL/许可证、排除已有用户配置和错误架构拒绝。此处 exe 为 fixture，不代表 Windows 二进制已运行。
- `lanpath-app verify` 与 `@lanpath/client-core verify` 通过，覆盖桌面/Android 前端构建、类型、lint 和格式。
- 原有 RuleModeSwitch 未使用参数与两处格式问题已单独修复；保留已有 React Compiler 优化跳过与大 chunk 提示。

## 设备边界与后续验收

本次未触发远端 Windows 工作流，未在 Windows 设备执行真实 ZIP/NSIS，未做 Android 真机测试。
发布前需在 Windows x64/arm64 验证：完整解压启动（含中文/空格路径）、WebView2 存在/缺失、从快捷方式不同工作目录启动、订阅/主题重启保留、移动目录后启动核心与 TUN、只读位置报错及手动覆盖升级保留 data。
确认新安装的业务和 WebView 数据都在 `.lanpath`；安装版更新仍走签名 NSIS，便携「检查更新」打开发布页且不执行安装器。
旧 identifier WebView 目录不自动迁移，外观偏好需要重新设置；旧业务数据目录不复制或删除。
macOS WKWebView 不支持此目录 API，当前 Portable 分发范围为 Windows；Linux 只验证共享路径解析与 host 编译，不宣称 Linux 便携包已完成设备验收。
