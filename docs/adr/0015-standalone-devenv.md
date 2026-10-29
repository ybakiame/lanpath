# ADR-0015 独立 devenv 环境与 CI/CD 统一入口

状态：Accepted

## 背景与授权

用户要求将 Nix + devenv Flake 包装迁移为完整的 standalone devenv，并明确包含相关 CI/CD。
用户已清理宿主 Nix daemon 的过时代理变量；本次不再修改守护进程或系统网络配置。
本决定取代 ADR-0005 的环境入口及锁定机制，不改变其模块拆分、共用命令和安全边界。

## 决定

- 使用独立 devenv CLI 2.4.0，安装来源固定为 `cachix/devenv` 的
  `b904dcb51fe48c30db250038241507f60752f222`，本地与 CI/CD 使用相同来源。
  同时显式固定该提交的 `src/modules` 输入，避免 CLI 默认解析上游最新模块。
  Nix 仍是底层包管理器及 CLI 引导工具，不再维护项目 `flake.nix` 或 `flake.lock`。
- `devenv.yaml` 声明输入，唯一项目环境锁文件为 `devenv.lock`；`devenv.nix` 继续导入
  `nix/` 中已有工具、桌面、Android 和共用命令。主 nixpkgs 保留 26.05 的
  `a033e66ab94a6bedf2c3d58d93a36319d3cbbbaf`，Android 输入保留
  `b3c4566d2d946a76082ce9be34640d24f7eec477`。输入 URL 固定 revision，
  不把入口迁移或下载排错作为升级 Bun、Rust、Go、SDK/NDK/JDK 或传递输入的授权。
- Android SDK 交由内置 `android.enable` 集成按原有 android-nixpkgs 输入组装，
  固定平台 35/36、build-tools 35/36 和 NDK 28.0.13004108；Java 交由内置语言模块，
  固定 JDK 17，关闭额外 Java LSP，不启用全局 Gradle、Android Studio、模拟器或系统镜像。
  `nix/android.nix` 只保留项目 API 33 的 Rust/Go 桥变量、兼容消费者所需的路径别名，
  以及原有用户级 Android/debug keystore 位置。内置初始化生成的根级 `local.properties`
  与 `.android/` 属于忽略的开发缓存，不进入版本控制或产品配置。
- 从仓库根运行 `devenv shell`，非交互入口为 `devenv shell -- COMMAND ...`。
  已激活 shell 的 `lanpath-*` 命令仍自行切回 Git 根；CI 的 Android 构建步骤不再
  依赖旧 Flake 的 `../..` 寻址。可选 direnv 使用 CLI 自带 `direnvrc` 和 `use devenv`。
- Linux/Android CI、Tauri 发布清单处理及 GPUI 发布前载荷验证统一使用此入口。
  云端明确配置 `cache.nixos.org`、devenv 官方缓存及其公开验签密钥。
  Windows 已授权的原生 MSVC/Rust/Bun 构建仍保持原生；不改变 runner、触发条件、
  权限、签名身份、制品验证与上传/发布门禁，不触发工作流或实际发布。
- 进入环境只准备工具和声明的环境变量，不自动安装项目依赖、构建 AAR/应用、启动代理
  或签名。显式关闭 devenv git-hooks，Husky 保持唯一提交检查入口；Gradle 使用项目 wrapper。
  本地覆盖文件与生成缓存忽略，不提交宿主绝对路径或个人环境配置。

## 验证与运行边界

迁移须分别验证 CLI 版本、输入 revision、实际工具/Android 路径、共用命令的子目录行为，
以及 CI/CD 配置和实际调用语义。环境可以进入不能代替 Rust/前端/Android 构建或设备验收；
本地发布验证通过也不能代替 GitHub 托管 runner、签名、历史升级及 ARM64 原生验证。
历史测试记录保留当时的 Nix Flake 命令，不改写成 standalone 入口的运行证据。

## 本轮实测

- 独立 CLI 与模块均固定为 2.4.0 对应提交；新锁的主 nixpkgs、Android 与 git-hooks
  revision 及摘要与原输入一致，无本机 path 输入。实际环境为 Go 1.26.8、Bun 1.4.2、
  Rust/Cargo 1.98.0、JDK 17.0.20.1、NDK 28.0.13004108。
- 内置 Android 环境中已编译并运行 Java 17 程序；NDK 编译对象及 Go/CGo 共享库
  均验证为 AArch64，后者具备预期 C ABI 导出。平台 35/36 与 build-tools 35/36
  元数据保持一致；使用 API 36 实际编译/链接资源并核对包名、minSdk 33 和应用标签。
  这不是完整产品 AAR/APK、模拟器或真机运行验收，也未生成或安装产品证书。
- `devenv shell -- lanpath-verify-rust` 完整通过 workspace 构建、fmt、严格 Clippy、
  888 项单元测试、3 项集成测试、1 项文档测试，以及独立 Tauri host Clippy 和 Android
  target check；4 项显式忽略的副作用用例/文档示例未执行，原有条件编译警告未抑制。
  Windows 条件夹具随后仅重跑引擎定向测试和原生检查，没有为文档/夹具重复完整门禁。
- 四个环境消费者工作流通过 actionlint；从子目录调用共用工具测试命令通过 10 项用例。
  实际 standalone CLI 保留发布 heredoc 的 stdin 与环境变量，并传播 `pipefail` 的非零退出码。
  没有远程触发 CI/CD、注入真实发布签名材料或发布制品。
- Windows 原生 common/engine 严格 Clippy、句柄所有权回归及 GPUI 严格 Clippy/构建通过；
  两个 macOS target 的共享测试代码通过交叉 Clippy，不能替代 macOS 原生运行。
  普通权限真实 GPUI/核心完成缓存预览、流量路由、缺缓存阻止与正常关闭的隔离验收，
  不把它作为 TUN、系统代理、UAC、停止分类或最终 UI 验收。

环境恢复期间，现有 Rust 工具链的链接器包装器仍引用已缺失的旧 Nix 包。
从官方签名缓存恢复准确依赖并保留与该工具链关联的间接 GC root，未重装或升级 Rust。
当前 Android CLI 的不可变 SDK 管理边界见[环境指南](../../devenv.md)；不修改 store 权限绕过。

## 分阶段集成

独立环境提交只承载仓库原有 CI、Android 构建和 Tauri 发布工作流。
GPUI 专用发布工作流已在工作区同步迁移并通过本轮检查，但依赖的发行工具尚属于未合并的
实验改动组，须与该实现一起提交，不在环境切换提交中提前激活。README、架构与开发文档的
环境段落同样单独提交，不把累积的 GPUI 产品迁移当成此次环境改造的一部分。
