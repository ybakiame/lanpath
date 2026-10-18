---
name: lanpath-environment
description: 准备或排查 Lanpath 本地开发环境，涵盖 Nix/devenv、锁定依赖、Android 工具链和下载故障。用于首次配置、环境缺失或工具链异常，不用于升级依赖或发布。
---

# Lanpath 环境准备

## 先确认项目事实

从当前检出根目录读取 [AGENTS.md](../../../AGENTS.md) 和 [开发文档](../../../docs/development.md) 的环境、Android 章节。
工具及命令以 [devenv.nix](../../../devenv.nix)、[nix](../../../nix/) 和 [flake.lock](../../../flake.lock) 为准；不要把本 skill 中的示例当成版本来源。

检查工作区状态、当前平台、Nix 是否可用，以及任务需要桌面还是 Android。不读取或输出私钥、个人配置和完整环境变量。
公开源码构建可跳过私有协议子模块；不要把无法访问协议仓库误判为构建依赖缺失。

## 准备所需环境

在仓库根目录进入环境：

```bash
nix develop --no-pure-eval
```

非交互执行可使用 `nix develop --no-pure-eval --command <命令> [参数...]`。
后续本地编译、运行及项目验证都在该环境中执行；进入 shell 本身不会安装依赖、构建 AAR 或启动代理。

- 需要前端依赖时执行 `lanpath-install`，使用根 Bun workspace 和锁文件。
- 需要 Android Rust target 时执行 `lanpath-rust-targets`，并检查 SDK/NDK/JDK 环境；不使用宿主 Gradle，使用项目 wrapper。
- 需要运行或打包 Android 时，确认当前检出的 Go 桥已生成对应 AAR；构建顺序和验证范围见 [构建验证 skill](../lanpath-build-verify/SKILL.md)。
- `.envrc` 的 direnv 支持是可选入口，不额外创建 devenv.yaml/devenv.lock。

若宿主缺少 Nix，先说明缺少的前提和对应平台的准备方式；安装系统组件、修改宿主配置应遵循当前任务授权，不通过安装宿主 Bun/Rust/Go 绕过项目环境。
Windows CI 使用原生锁定工具链是已有 CI 例外，不能扩展成本地开发例外。

## 下载或工具链异常

先区分失败发生在 Nix 求值/缓存、源码下载、Bun、Cargo、Go 还是 Gradle，保留脱敏错误和失败命令。
核对工具来自本仓环境，锁文件保持原样；不要为解决下载失败更新锁文件、切换 channel 或关闭校验。

Nix 链接/下载失败时，可按 AGENTS 的既有方案在当前命令环境设置代理后重试。
`http://127.0.0.1:7899` 只在代理实际监听于当前网络命名空间时有效；WSL 中的 localhost 不一定指向 Windows 代理。
先验证地址连通性，不盲目修改 Windows/WSL 网络模式、防火墙或系统代理，也不把临时代理写入仓库。
代理不可达时停止相同重试，报告网络阻塞，不归因于项目代码。

## 交付信息

说明准备的目标、已成功的命令、仍缺的依赖或网络条件。区分“环境可进入”“依赖可解析”和“应用已构建”；只有实际执行的结果才算验证。
