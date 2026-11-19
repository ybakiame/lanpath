# AGENTS.md — Lanpath 开发指南

## 环境

standalone devenv 2.4.0 的固定安装入口见 [环境指南](devenv.md)；`devenv.yaml` 声明输入，`devenv.lock` 是唯一项目环境锁文件。仅在明确的工具链更新任务中修改输入并使用 `devenv update`；不因排错擅自更新锁定版本。

### 本地环境

Linux：本地编译/运行一律先从仓库根进入 `devenv shell`；非交互使用 `devenv shell -- COMMAND ...`。Go、Bun、Rust、SDK/NDK/JDK 由本仓环境提供。Gradle 使用项目 wrapper，不使用宿主 Gradle。
Windows：使用原生 Windows Rust/MSVC/CMake 工具链

### CI 环境

Windows CI 使用锁定版本的原生 MSVC/Rust/Bun，Linux/Android CI 继续 Nix。


## 编译与验证

Rust 执行 cargo build --workspace 和 bun run verify:rust，含根 fmt/clippy/test、
桌面壳 host clippy 与有 SDK/target 时 Android target check；根 workspace 不覆盖壳。
根门禁不覆盖 apps/gpui、apps/nsh 或 apps/desktop；其改动另执行独立项目 fmt/clippy/test/build 及
Windows 原生场景，NSH 按目标增加 nsh test、Android 打包与获准的真机验证，不据此新增强制 CI 门禁。

### tauri
前端改动执行受影响包 verify：lanpath-app、@lanpath/client-core、@lanpath/ui。
前端无单元测试运行器，verify 为 build+lint+format；交互验证注明 mock/设备边界。

### android
Go 调整还需构建 lanpathcore.aar，并编译 Android Kotlin/资源


## 结构与边界

apps/client 为 Tauri 单壳双目标应用；页面共用 @lanpath/ui，平台为编译期事实。
packages/client-core 封装 API/hooks/atoms；UI 禁止直接 invoke。
根 Cargo workspace 为 crates/lanpath-*；apps/client/src-tauri 仍是独立 Cargo 项目。
Android 使用 apps/client/lanpath-core 的 Go libbox 绑定，无 MITM；现行 Tauri 前端保持 sing-box。

apps/gpui 为 GPUI 实验性目标，支持 sing-box 与 Mihomo(仅运行) ，保持独立 Cargo 项目与独立应用身份；复用已有 Rust 核心基础。
实验阶段仅支持便携安装模式，方便数据隔离与测试。

apps/nsh 为 Rust NSH 实验性目标，支持 sing-box 与 Mihomo(仅运行) ，保持独立 Cargo 项目与独立应用身份；复用已有 Rust 核心基础。
实验阶段仅支持便携安装模式，方便数据隔离与测试。
Android 后续仍复用 Kotlin VpnService 与 Go/libbox。

apps/desktop(nsh) 为 `nsh-validation` 分支上的 NSH Windows 迁移目标：纯 Rust NSH 0.5.1、独立 Cargo
项目与独立应用身份 `com.ybakiame.lanpath.desktop.nsh`，不读取/迁移 Tauri/GPUI/apps/nsh 数据；
核心仅做用户态二进制校验与启停（mixed/Clash API 限 loopback）。Windows 系统代理经本任务授权的
独立普通权限同映像守护精确接管/恢复（owner 为本数据根 UUID，GUI 侧绝不直写注册表；接管失败
保留核心与恢复证据，停止先守护恢复再停核心）；不接管 TUN/VPN，不安装服务/启动项，不经 TUN
helper。

## 编码与提交

Rust 使用 LanpathError/LanpathResult；除测试及初始化外禁止裸 unwrap/expect。
一个提交对应一个逻辑变更；使用 Conventional Commits，不绕过 pre-commit。
业务文件上限 500 行、测试 1000 行，超线按职责拆分。
使用 rg 搜索，依赖在根目录 Bun workspace 统一安装，不引入多锁文件。

## 工作流与完成定义

开发方案、设计稿、测试记录、临时分析和工作日志统一放在 `.work/`（已忽略），不写入 `docs/` 或应用目录。
README、CHANGELOG 和发布说明仅保留面向使用者的内容。
每轮包含发现、计划、实现、验证、文档、最终复查。完成须满足请求、相关检查通过、
行为变更记录 CHANGELOG、文档同步、diff 无无关变更、提交原子化；未执行的设备验证明确记录。

遇到权威冲突、未经决定的架构/公开接口变化、新强制门禁、数据迁移/删除 或不可确定需求时停下请求决策。
不得将签名私钥、配置、个人数据、生成产物或宿主绝对路径提交。
