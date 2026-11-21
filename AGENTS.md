# AGENTS.md — Lanpath 开发指南

## 环境

standalone devenv 2.4.0 的固定安装入口见 [环境指南](devenv.md)；`devenv.yaml` 声明输入，`devenv.lock` 是唯一项目环境锁文件。仅在明确的工具链更新任务中修改输入并使用 `devenv update`；不因排错擅自更新锁定版本。

### 本地环境

Linux：本地编译/运行一律先从仓库根进入 `devenv shell`；非交互使用 `devenv shell -- COMMAND ...`。Go、Bun、Rust、SDK/NDK/JDK 由本仓环境提供。Gradle 使用项目 wrapper，不使用宿主 Gradle。
Windows：使用原生 Windows Rust/MSVC/CMake 工具链

### CI 环境

Windows CI 使用锁定版本的原生 MSVC/Rust/Bun，Linux/Android CI 继续 Nix。


## 编译与验证

根 Rust workspace 按需执行 `cargo build --workspace` 或 `bash scripts/check-rust-gates.sh`。
根门禁不覆盖独立的 `apps/desktop`；按改动执行对应 fmt/clippy/test/build 和 Windows 原生验证。
Android 原生库改动构建 `mobile/android`；Go 绑定改动先重新构建 `lanpathcore.aar`，再编译 Kotlin/资源。
库构建通过不代表移动端宿主或 VPN 真机接入通过，不据此新增强制 CI 门禁。

## 结构与边界

`apps/desktop` 为当前 NSH 桌面目标。共享 Rust 业务位于 `crates/lanpath-*`，公共资源位于 `assets/`。
`mobile/lanpath-core` 封装 Go/libbox 绑定；`mobile/android` 为独立 Android 原生库工程，保留 Kotlin VpnService、网络接口、通知、日志和文件导出。移动端仅支持 sing-box，无 MITM。
移动端暂未接入新的应用宿主/页面，不提供 APK；本次拆分不代表 NSH 已接入 VPN。
旧 Tauri client、前端 packages 和 Tauri 命令适配层已移除，不恢复其构建入口或发布流程。

Android 后续仍复用 Kotlin VpnService 与 Go/libbox。

`apps/desktop` 为 `nsh` 分支上的正式 NSH 桌面目标：纯 Rust NSH 0.6.0、独立 Cargo
项目与独立应用身份 `com.ybakiame.lanpath.desktop.nsh`，不读取/迁移旧版客户端数据；
核心仅做用户态二进制校验与启停（mixed/Clash API 限 loopback）。Windows 系统代理经本任务授权的
独立普通权限同映像守护精确接管/恢复（owner 为本数据根 UUID，GUI 侧绝不直写注册表；接管失败
保留核心与恢复证据，停止先守护恢复再停核心）；不接管 TUN/VPN，不安装服务/启动项，不经 TUN
helper。

## 编码与提交

Rust 使用 LanpathError/LanpathResult；除测试及初始化外禁止裸 unwrap/expect。
一个提交对应一个逻辑变更；使用 Conventional Commits，不绕过 pre-commit。
业务文件上限 500 行、测试 1000 行，超线按职责拆分。
使用 rg 搜索；仍使用的 Bun 工具依赖在根目录统一安装，不引入多锁文件。

## 工作流与完成定义

开发方案、设计稿、测试记录、临时分析和工作日志统一放在 `.tmp/work/`（已忽略），不写入 `docs/` 或应用目录。
README、CHANGELOG 和发布说明仅保留面向使用者的内容。
每轮包含发现、计划、实现、验证、文档、最终复查。完成须满足请求、相关检查通过、
行为变更记录 CHANGELOG、文档同步、diff 无无关变更、提交原子化；未执行的设备验证明确记录。

遇到权威冲突、未经决定的架构/公开接口变化、新强制门禁、数据迁移/删除 或不可确定需求时停下请求决策。
不得将签名私钥、配置、个人数据、生成产物或宿主绝对路径提交。

<!-- precept:begin v1 -->
## Precept

This project's confirmed engineering context (the developer's reviewed preferences and the repository's contracts) is served by Precept. Before planning or editing, call `get_effective_context` on the `precept-lanpath` MCP server, and again after your context is compacted. Current repository files and contracts outrank those preferences: report a conflict instead of choosing silently.
<!-- precept:end -->
