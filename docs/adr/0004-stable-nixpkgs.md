# ADR-0004 主开发环境迁至稳定 Nixpkgs

状态：Accepted · 日期：2026-10-15

## 背景

用户决定逐步从 unstable 迁至 26.05，并在后续阶段引入 devenv 整理环境与 CI。先独立验证主源兼容性，避免环境组织和依赖版本同时变化。

## 决定

- 主 nixpkgs 改用 nixos-26.05，flake.lock 固定具体提交；保留 nix develop 入口和现有 CI 命令。
- 26.05 本次锁定快照的 Bun 为 1.3.13，低于项目和 Windows CI 的 1.4.2；使用稳定源打包定义和官方 x64 baseline 归档，固定 1.4.2 与原锁定 SHA256，不引入额外完整 unstable 输入。
- Rust 1.98.0、NDK 28.0.13004108、Android 平台 35/36 与 build-tools 35/36 的约束保持。Gradle 使用 wrapper，不使用 shell 中的宿主 Gradle。
- Android SDK 输入及其传递 nixpkgs 保持原锁定快照；本阶段只迁移主源，不宣称整个锁定图已移除 unstable。
- 本地 Linux、Android 和 Linux CI 使用同一主源；Windows 原生构建例外保持原方案。

## 后果

锁文件保证具体依赖提交固定；稳定分支主要改善更新策略，不代表任意更新无需验证。26.05 支持期至 2026-12-31，后续稳定分支更新另行验证。
固定 Bun 需要显式维护版本和摘要；devenv 接入及 SDK 输入整理作为后续独立变更，不创建并行锁文件或第二套工具链。

依据：[NixOS 26.05 发布说明](https://nixos.org/blog/announcements/2026/nixos-2605/)。
