# Lanpath 开发环境（standalone devenv）

本仓库使用独立的 devenv CLI 管理 Nix 开发环境，不再以项目 Flake 包装 shell。`devenv.yaml` 声明输入，`devenv.lock` 是唯一项目环境锁文件，`devenv.nix` 导入 `nix/` 下的工具、桌面、Android 和共用命令模块。

本地编译、运行、测试及提交钩子使用该环境。Windows 原生工具链使用约定见 [AGENTS.md](AGENTS.md)。本说明的 Linux/WSL 环境不构成新增平台支持承诺。

## 1. 前置：Nix 与固定的 devenv CLI

先按 [Nix 官方安装说明](https://nixos.org/download/) 为宿主准备 Nix；系统组件安装及宿主配置变更遵循当前任务授权。以下 CLI 引导使用 Nix 的 `nix-command` 和 `flakes` 特性，这不代表项目仍维护 Flake。确保 Nix 与用户 profile 的可执行目录已在 `PATH` 中。

固定安装 devenv **2.4.0**，不用未固定的分支或 `latest`：

```sh
nix profile add --accept-flake-config github:cachix/devenv/b904dcb51fe48c30db250038241507f60752f222#devenv
```

CLI 自身的构建依赖与项目输入分别锁定；安装 CLI 不升级本仓 Bun、Rust、Go 或 Android 工具。现有 Nix daemon/系统代理配置不属于项目环境迁移，不因切换入口改动。
`devenv.yaml` 也显式固定同一提交的 `src/modules` 输入；只固定 CLI 而省略模块输入，
会让 CLI 默认解析上游当前模块，不能视为维持原有环境版本。

## 2. 使用

从仓库根目录执行：

```bash
devenv shell
# 在已激活 shell 中，按任务显式准备依赖、运行或验证：
lanpath-install
exit
```

非交互命令同样从仓库根执行：

```bash
devenv shell -- cargo check --manifest-path apps/desktop/Cargo.toml
devenv shell -- bash -c 'adb devices; go version; bun --version'
```

已激活 shell 中的 `lanpath-*` 共用命令会自行切回 Git 根，因此可以从仓库子目录调用。可选的 direnv 入口使用已提交的 `.envrc`，审核后运行 `direnv allow`；不要写回 `use flake` 或另建项目环境锁文件。

进入 shell 只准备环境，不自动安装项目依赖、构建 AAR/应用、启动代理、签名或发布。Husky 保持唯一提交检查入口，不启用 devenv git-hooks。构建顺序与验证范围见 [AGENTS.md](AGENTS.md)。

## 3. 覆盖的依赖

| 类别 | 内容 | 来源 |
|---|---|---|
| Android | SDK 35/36、build-tools 35.0.0/36.0.0、platform-tools、NDK 28.0.13004108、cmdline-tools；许可证按项目配置接受 | 内置 `android.enable` 与固定的 `tadfisher/android-nixpkgs` 输入 |
| Android 工具链 | JDK 17、cargo-ndk；Gradle 仅使用项目 wrapper，不安装全局 Gradle | 内置 `languages.java` 固定 JDK，其他工具来自固定 nixpkgs |
| Rust | Nix 提供 rustup；Rust 版本、components 与 target 以 `rust-toolchain.toml` 为准 | rustup |
| Go | 固定 nixpkgs 提供的 Go（当前 1.26.8；lanpathcore 要求 ≥1.25.5） | 固定 nixpkgs |
| 工具脚本 | Bun 1.4.2 官方归档与 SHA256 固定；Node.js；根清单与锁文件仅管理仍使用的打包/提交工具 | `nix/bun.nix` 与固定 nixpkgs |
| 桌面 GUI 系统库 | webkit2gtk-4.1、gtk3、glib-networking、openssl、alsa-lib、xorg 等，供 NSH/GPUI 的 Linux 图形、音频和可选 WebView/媒体能力使用 | 固定 nixpkgs |
| 构建基础 | Python 3、pkg-config、cmake、ninja、perl、gnumake 等 | 固定 nixpkgs |

SDK 组装与基本路径由 [devenv 内置 Android 集成](https://devenv.sh/integrations/android/) 管理，
`nix/android.nix` 只保留 API 33 的 Cargo/Go 桥变量、消费者所需路径别名和原有用户级
Android/debug keystore 位置。固定 JDK 17，关闭额外 Java LSP、模拟器、系统镜像与 Android Studio。
内置初始化生成的根级 `local.properties` 与 `.android/` 已忽略，不是可提交的产品配置。
不要在宿主 shell rc 中无条件覆盖项目 SDK；遇到路径异常先区分宿主与项目环境。
SDK 包增删走声明配置和输入锁定，不在不可变的 Nix store 中运行命令原地安装或更新。
当前 cmdline-tools 的 `sdkmanager --list_installed` 也会转交新 Android CLI 并尝试写 SDK 内的
`.sdk`，在 Nix store 下会拒绝写入；组件核对使用安装元数据与实际编译/资源工具，不据此更改 store 权限。

`lanpath-build-aar` 使用当前检出 `.devenv/state/gomobile/<fork版本>/<Go版本>/bin`
下的 `gomobile`/`gobind` 配对工具，缺失时按脚本固定版本安装，不借用或覆盖共享 GOPATH
中的已安装工具。工具升级仍是独立任务；不能因共享缓存存在更新版本而静默采用。
绑定构建保留包级进度和错误输出，不开启可能打印完整进程环境的 `-x` 跟踪。

首次准备可能下载 Android SDK/NDK、CLI 辅助程序和其他 Nix 构建依赖。项目配置使用 `cache.nixos.org` 与 devenv 官方签名缓存；缓存未命中时仍可能从源码构建。standalone CLI 管理输入锁定与求值缓存，但实际命中、下载量及耗时取决于配置和缓存状态，不承诺固定体积、时长或编译加速。

WSL GUI 仍依赖现有 WSLg/宿主图形能力；ADB 操作前先通过 `adb devices` 核对目标设备和授权状态，不因进入环境擅自重启宿主 ADB server。

## 4. 输入与工具更新边界

本次迁移保留主 nixpkgs 26.05 的 `a033e66ab94a6bedf2c3d58d93a36319d3cbbbaf`、Android 输入 `b3c4566d2d946a76082ce9be34640d24f7eec477`，以及既有工具版本。输入声明与项目锁定只使用 `devenv.yaml`/`devenv.lock`，不恢复 `flake.nix`/`flake.lock`。

只有明确的更新任务才修改输入 revision，再定向更新对应锁定节点，例如：

```bash
# 先在 devenv.yaml 中明确选择获准的 nixpkgs revision，再从仓库根执行：
devenv update nixpkgs
```

`devenv update` 不带输入名会更新全部输入，不用于下载排错或顺带升级。固定 revision 不会因执行 update 自动变成新分支。Android 输入、SDK/NDK 组件、Bun 版本/摘要、`rust-toolchain.toml` 与 CLI 安装 revision 各有独立变更边界；更新 CLI 时也须同步本地与 CI/CD 的固定安装命令，不以项目输入更新替代 CLI 升级。

更新后按影响范围核对锁定图和实际工具版本，再完成对应 Rust、Android/桌面及 CI/CD 验证；环境可进入不代表应用已构建或发布路径已验证。历史验证记录保留当时命令，不能作为 standalone 入口的新证据。

上游参考：[输入与锁定](https://devenv.sh/inputs/)、[GitHub Actions 集成](https://devenv.sh/integrations/github-actions/)。
