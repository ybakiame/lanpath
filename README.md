# 岚径 · Lanpath

跨平台代理客户端：订阅、代理策略、DNS/规则、流量与日志诊断。

当前桌面开发目标是 `apps/desktop`（Rust / NSH 0.6.0）；`apps/gpui` 保留为独立对照目标。共享业务位于 `crates/lanpath-*`，公共图标与规则目录位于 `assets/`。

## 开发

Linux 本地构建统一使用本仓 standalone devenv（底层为 Nix）。安装固定版本的方式见 [环境指南](devenv.md)。Windows 桌面使用原生 Rust/MSVC/CMake 工具链。

```bash
devenv shell -- cargo run --manifest-path apps/desktop/Cargo.toml
```

根 Bun 工具依赖只用于 GPUI 打包工具和提交检查，使用 `devenv shell -- bun install --frozen-lockfile` 安装。

## 移动端

可复用的 Android 原生库和 Go/libbox 绑定位于 [mobile/](mobile/README.md)，已脱离旧客户端。当前不提供可安装 APK，后续移动端宿主与 VPN 接线尚未完成。

```bash
devenv shell -- lanpath-build-aar
devenv shell -- lanpath-build-android-debug
```

## Windows GPUI 实验版

`apps/gpui` 是独立 GPUI 应用，程序为 `lanpath-gpui.exe`，不依赖 WebView。
仅分发签名 ZIP，默认便携：完整解压后保留旁边的空文件 `lanpath-gpui.portable`，
数据写入程序目录下的 `data/`，不受终端或快捷方式工作目录影响。
需要独立标准目录 `%LOCALAPPDATA%\LanpathGPUI` 时，正常退出后移除便携标记再重开；
不自动搬迁数据，也不读取或覆盖 Tauri 的原目录。

## Windows NSH 桌面实验版

`apps/desktop` 为当前 NSH 桌面目标。程序旁放置空文件 `lanpath.portable`，
业务数据及界面偏好使用同级 `data/`。不同验证目录可各放一份程序与标记文件。
使用方式见 [NSH 桌面说明](apps/desktop/README.md)。

本仓可独立构建，不需要 ProxyPanel 检出，不作为其 submodule。
[文档索引](docs/README.md) · [代理指南](AGENTS.md)。

## 许可与来源

AGPL-3.0-or-later，保留 ProxyPanel 与 Lanpath 贡献者归属。源码由 ProxyPanel 客户端整理而来；
服务端历史保留于原仓。
sing-box、wintun 等第三方组件的许可资产继续随分发保留。
