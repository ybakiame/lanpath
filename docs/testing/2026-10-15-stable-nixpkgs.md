# 主开发源稳定分支验证（2026-10-15）

主输入为 nixos-26.05，锁定 a033e66ab94a6bedf2c3d58d93a36319d3cbbbaf；Android SDK 输入及其传递快照未变。
Bun 官方归档 SHA256 与原锁定归档一致，实际运行仍为 1.4.2。工具版本：Go 1.26.8、Rust 1.98.0、Node 24.21.0、JDK 17.0.20.1、NDK 28.0.13004108。

全部验证在本仓 nix develop 环境运行：

- bun install --frozen-lockfile、app（desktop/Android 前端）verify、client-core verify、UI verify 通过。
- 9 项运行缓存/Portable/发布工具测试通过。
- cargo build --workspace 和 verify:rust 通过，含根 fmt/clippy/test、独立桌面壳 host clippy 与 Android target check。
- GOTOOLCHAIN=local 构建 lanpathcore.aar 通过，Go 工具链未自动切换。
- tauri android build --debug --target aarch64 通过，包含 Rust、Kotlin 和资源编译，输出 debug APK/AAB；产物不提交、未安装设备。

本机 daemon 的过期代理配置曾导致缓存下载超时，通过客户端预取受信任的官方缓存和验证固定归档完成环境准备；未修改主机服务配置。未进行新提交的云端 CI、Windows 编译或真机行为验收。
