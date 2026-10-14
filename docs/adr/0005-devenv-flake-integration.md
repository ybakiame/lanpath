# ADR-0005 devenv Flake 集成与共用命令

状态：Accepted · 日期：2026-10-15

## 背景

用户确认分阶段迁移至稳定 Nixpkgs 并引入 devenv。主源已独立完成双端构建验证。现有本地规范及 Linux/Android CI 使用 Flake；直接切换到 devenv CLI 会同时改变入口和锁定机制。

## 决定

- 使用 devenv v2.4.0 的 devenv.lib.mkShell，保留 nix develop --no-pure-eval 和唯一 flake.lock。devenv 输入 follows 主 nixpkgs，不引入另一份项目 devenv.lock。
- devenv.nix 只组织模块。nix/tools.nix、desktop.nix、android.nix 定义对应工具与环境，bun.nix 固定已验证的 Bun，commands.nix 定义本地/CI 共用脚本。
- Rust 仍使用 Nix 提供的 rustup 和 rust-toolchain.toml，不叠加第二套 Rust 环境；CI 的目标安装步骤保留。SDK/NDK/JDK 和交叉编译参数沿用已验证约束。
- 移除全局 Gradle，Android 始终使用 wrapper。复用 Husky，不启用 devenv 提交钩子。进入 shell 不安装项目依赖、不启动进程、不构建或签名。
- 共用脚本从 Git 工作区根执行，严格传播失败；签名配置及发布权限仍由原 release 工作流负责。Windows 原生构建例外不变。
- 显式使用 cache.nixos.org 和 devenv 官方缓存及其公开验签密钥。devenv 任务辅助程序按上游自带锁定快照构建，项目工具仍按 26.05；不把辅助构建快照视为项目工具链切换。

## 后果

Flake 集成需要非纯评估以确定运行目录；它不具备 dedicated CLI 的全部缓存特性。本次收益是配置按职责拆分、验证命令复用和减少 CI 重复进入环境，不承诺具体编译加速。
独立性能缓存和进一步的环境裁剪须根据测量另行实施。上游辅助程序缓存缺失时仍可从源码构建，初次环境准备可能较慢。

依据：[devenv Flake 集成](https://devenv.sh/guides/using-with-flakes/)、[GitHub Actions 集成](https://devenv.sh/integrations/github-actions/)。
