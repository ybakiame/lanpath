# 岚径 · Lanpath

跨平台代理客户端：订阅、代理策略、DNS/规则、流量与日志诊断。

Windows 提供 x64/arm64 安装版与 Portable；Android 提供 arm64 release 签名 APK（Android 13+）。

## 开发

本地构建、运行和资源生成统一使用本仓 standalone devenv（底层为 Nix）。先按 [环境指南](devenv.md) 安装固定的 devenv 2.4.0；Windows CI 原生工具链例外由维护者明确授权，版本锁定。

~~~bash
devenv shell
bun install --frozen-lockfile
bun run --filter lanpath-app verify
bun run verify:rust
cd apps/client
bun run tauri dev
~~~

Android：先在 Nix 环境运行 bash apps/client/scripts/build-lanpath-core.sh，
然后进入 apps/client 运行 bun run tauri android dev --target aarch64。
前端热更新用于调试，不需要反复生成 APK。构建约定见 [AGENTS.md](AGENTS.md)。

非交互命令从仓库根执行 `devenv shell -- COMMAND ...`；项目输入由 `devenv.yaml` 声明并仅由 `devenv.lock` 锁定。

首次 Android 安装采用新包名 com.ybakiame.lanpath，与旧包不能覆盖升级；不要先卸载旧包。
桌面新安装使用 ~/.lanpath，已有旧数据且新目录不存在时沿用 ~/.proxy-panel-client。
Windows/Linux 的 WebView 偏好和缓存使用该目录的 `webview` 子目录，旧 identifier 目录保留，不自动迁移。

Windows 提供 x64/arm64 Portable ZIP：完整解压后运行 `Lanpath.exe`，保留旁边的 `lanpath.portable` 标记，所有应用数据使用程序目录下的 `data`。启动目录不影响数据位置；需要系统已安装 WebView2 Runtime。更新时退出应用、替换程序与 `seed`，保留 `data`。

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
