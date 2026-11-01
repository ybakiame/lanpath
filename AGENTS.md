# AGENTS.md — Lanpath 开发指南

## 权威与协议

项目通过 .protocol/agent-development-protocol 引入协议。协议采纳/维护任务先读取其中 ADOPT.md。
本仓 README、AGENTS 与 docs 是项目权威；通用协议不能替代项目知识。
普通开发不修改协议子模块。遵循 [采纳记录](docs/adoption/adoption-report.md)。

## 结构与边界

现行发行目标 apps/client 为 Tauri 单壳双目标应用；页面共用 @lanpath/ui，平台为编译期事实。
packages/client-core 封装 API/hooks/atoms；UI 禁止直接 invoke。
根 Cargo workspace 为 crates/lanpath-*；apps/client/src-tauri 仍是独立 Cargo 项目。
Android 使用 apps/client/lanpath-core 的 Go libbox 绑定，无 MITM；现行 Tauri 前端保持 sing-box。
GPUI 实验桌面按 ADR-0017 支持 sing-box 与 Mihomo：共享库存/配置合成按核心类型分派，Android 拒绝 Mihomo。
不依赖 ProxyPanel 源码，不把协议框架写成产品实现。

apps/desktop 为 ADR-0013 授权的 Windows GPUI 实验性并行目标，保持独立 Cargo 项目；
现行 Tauri 与 Android 不被替换。GPUI 复用共享 Rust 业务，UI 状态由对应 feature/entity 持有。
实验身份、数据根、核心进程与端口独立，不自动读取或迁移生产数据，不接管系统代理/TUN，
不改变现行安装、更新和签名身份。后续生产切换或全局网络控制须单独取得决策。

## 环境和验证

按任务读取仓库 skills，不要求工具自动发现：

- 环境准备、依赖或下载排错：[lanpath-environment](.agents/skills/lanpath-environment/SKILL.md)。
- 按改动范围选择构建、测试与行为验收：[lanpath-build-verify](.agents/skills/lanpath-build-verify/SKILL.md)。
- ADB 已连接设备上的原生行为调试：[lanpath-android-debug](.agents/skills/lanpath-android-debug/SKILL.md)。

Skills 为操作指引，不替代本文件、docs 或实际脚本；不增加门禁或扩大任务授权。

本地编译/运行一律先从仓库根进入 `devenv shell`；非交互使用 `devenv shell -- COMMAND ...`。Go、Bun、Rust、SDK/NDK/JDK 由本仓环境提供。
Gradle 使用项目 wrapper，不使用宿主 Gradle。现行应用的环境例外：用户已授权 Windows CI
使用锁定版本的原生 MSVC/Rust/Bun，Linux/Android CI 继续 Nix。
ADR-0012/0013 另授权上述 GPUI 实验目标在本机使用现有原生 Windows Rust/MSVC/CMake；
该例外不授权安装或升级宿主组件，不扩展到其他应用或发布路径。
standalone devenv 2.4.0 的固定安装入口见 [环境指南](devenv.md)；`devenv.yaml` 声明输入，`devenv.lock` 是唯一项目环境锁文件。仅在明确的工具链更新任务中修改输入并使用 `devenv update`；不因排错擅自更新锁定版本。见 [ADR-0015](docs/adr/0015-standalone-devenv.md)。
代理可在 Nix 链接失败时设置 http_proxy/https_proxy 为 http://127.0.0.1:7899 后重试。

前端改动执行受影响包 verify：lanpath-app、@lanpath/client-core、@lanpath/ui。
前端无单元测试运行器，verify 为 build+lint+format；交互验证注明 mock/设备边界。
Rust 执行 cargo build --workspace 和 bun run verify:rust，含根 fmt/clippy/test、
桌面壳 host clippy 与有 SDK/target 时 Android target check；根 workspace 不覆盖壳。
根门禁不覆盖 apps/desktop；其改动另执行独立项目 fmt/clippy/test/build 及 Windows 原生场景，
不据此新增强制 CI 门禁。
Go 桥调整还需构建 lanpathcore.aar，并编译 Android Kotlin/资源；不得沿用旧品牌 AAR。

## 编码与提交

保留既有 .agents/rules 中的代码组织、状态管理和包管理规范。
Rust 使用 LanpathError/LanpathResult；除测试及初始化外禁止裸 unwrap/expect。
一个提交对应一个逻辑变更；使用 Conventional Commits，不绕过 pre-commit。
业务文件上限 500 行、测试 1000 行，超线按职责拆分。
使用 rg 搜索，依赖在根目录 Bun workspace 统一安装，不引入多锁文件。

## 工作流与完成定义

feature、bugfix、refactor、docs、adoption 按 [开发工作流](docs/development.md) 执行。
每轮包含发现、计划、实现、验证、文档、最终复查。完成须满足请求、相关检查通过、
行为变更记录 CHANGELOG、文档同步、diff 无无关变更、提交原子化；未执行的设备验证明确记录。
架构/发布身份/构建边界变化须记录 ADR；不得仅凭主机通过宣布移动目标正确。

遇到权威冲突、未经决定的架构/公开接口变化、新强制门禁、数据迁移/删除、
协议子模块改动或不可确定需求时停下请求决策；厂商特有要求只放 adapters。
不得将签名私钥、配置、个人数据、生成产物或宿主绝对路径提交。
