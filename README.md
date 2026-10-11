# 岚径 · Lanpath

跨平台代理客户端：订阅、代理策略、DNS/规则、流量与日志诊断；桌面额外提供 MITM 和脚本。
桌面及 Android 共用 Tauri 2 + React UI，当前仅使用 sing-box 运行核心。

## 开发

本地构建、运行和资源生成统一使用本仓 Nix 配置。Windows CI 原生工具链例外由维护者明确授权，版本锁定。

~~~bash
git submodule update --init --recursive
nix develop
bun install --frozen-lockfile
bun run --filter lanpath-app verify
bun run verify:rust
cd apps/client
bun run tauri dev
~~~

Android：先在 Nix 环境运行 bash apps/client/scripts/build-lanpath-core.sh，
然后进入 apps/client 运行 bun run tauri android dev --target aarch64。
前端热更新用于调试，不需要反复生成 APK。完整说明见 [开发指南](docs/development.md)。

首次 Android 安装采用新包名 com.ybakiame.lanpath，与旧包不能覆盖升级；不要先卸载旧包。
桌面新安装使用 ~/.lanpath，已有旧数据且新目录不存在时沿用 ~/.proxy-panel-client。
Windows/Linux 的 WebView 偏好和缓存使用该目录的 `webview` 子目录，旧 identifier 目录保留，不自动迁移。

Windows 提供 x64/arm64 Portable ZIP：完整解压后运行 `Lanpath.exe`，保留旁边的 `lanpath.portable` 标记，所有应用数据使用程序目录下的 `data`。启动目录不影响数据位置；需要系统已安装 WebView2 Runtime。更新时退出应用、替换程序与 `seed`，保留 `data`。详见 [便携版说明](docs/operations.md)。
迁移与发布限制见 [运维与发布](docs/operations.md)。

本仓可独立构建，不需要 ProxyPanel 检出，不作为其 submodule。
[文档索引](docs/index.md) · [代理指南](AGENTS.md) · [来源记录](docs/source-provenance.json)。

## 许可与来源

AGPL-3.0-or-later，保留 ProxyPanel 与 Lanpath 贡献者归属。源码由 ProxyPanel 客户端整理而来；
服务端历史保留于原仓，来源基线与工作区变更记录见来源记录。
sing-box、wintun 等第三方组件的许可资产继续随分发保留。
