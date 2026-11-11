# 岚径 · Lanpath

跨平台代理客户端：订阅、代理策略、DNS/规则、流量与日志诊断；桌面额外提供 MITM 和脚本。
现行发行桌面及 Android 共用 Tauri 2 + React UI；Tauri/Android 前端保持 sing-box。
GPUI 实验版按 ADR-0017 支持 sing-box 与 Mihomo 核心切换。

公开仓库：[ybakiame/lanpath](https://github.com/ybakiame/lanpath) ·
首次测试版：[v0.1.1-beta.1](https://github.com/ybakiame/lanpath/releases/tag/v0.1.1-beta.1)。
Windows 提供 x64/arm64 安装版与 Portable；Android 提供 arm64 release 签名 APK（Android 13+）。

## 开发

本地构建、运行和资源生成统一使用本仓 standalone devenv（底层为 Nix）。先按 [环境指南](devenv.md) 安装固定的 devenv 2.4.0；Windows CI 原生工具链例外由维护者明确授权，版本锁定。
协议子模块为私有治理资源，维护者需单独访问权限；公开源码编译可跳过下面的子模块初始化。

~~~bash
git submodule update --init --recursive
devenv shell
bun install --frozen-lockfile
bun run --filter lanpath-app verify
bun run verify:rust
cd apps/client
bun run tauri dev
~~~

Android：先在 Nix 环境运行 bash apps/client/scripts/build-lanpath-core.sh，
然后进入 apps/client 运行 bun run tauri android dev --target aarch64。
前端热更新用于调试，不需要反复生成 APK。完整说明见 [开发指南](docs/development.md)。

非交互命令从仓库根执行 `devenv shell -- COMMAND ...`；项目输入由 `devenv.yaml` 声明并仅由 `devenv.lock` 锁定。入口与锁文件迁移见 [ADR-0015](docs/adr/0015-standalone-devenv.md)。

首次 Android 安装采用新包名 com.ybakiame.lanpath，与旧包不能覆盖升级；不要先卸载旧包。
桌面新安装使用 ~/.lanpath，已有旧数据且新目录不存在时沿用 ~/.proxy-panel-client。
Windows/Linux 的 WebView 偏好和缓存使用该目录的 `webview` 子目录，旧 identifier 目录保留，不自动迁移。

Windows 提供 x64/arm64 Portable ZIP：完整解压后运行 `Lanpath.exe`，保留旁边的 `lanpath.portable` 标记，所有应用数据使用程序目录下的 `data`。启动目录不影响数据位置；需要系统已安装 WebView2 Runtime。更新时退出应用、替换程序与 `seed`，保留 `data`。详见 [便携版说明](docs/operations.md)。
迁移与发布限制见 [运维与发布](docs/operations.md)。

## Windows GPUI 实验版

`apps/gpui` 是独立 GPUI 应用，程序为 `lanpath-gpui.exe`，不依赖 WebView。
仅分发签名 ZIP，默认便携：完整解压后保留旁边的空文件 `lanpath-gpui.portable`，
数据写入程序目录下的 `data/`，不受终端或快捷方式工作目录影响。
需要独立标准目录 `%LOCALAPPDATA%\LanpathGPUI` 时，正常退出后移除便携标记再重开；
不自动搬迁数据，也不读取或覆盖 Tauri 的原目录。具体切换、更新与恢复见
[开发指南](docs/development.md#windows-gpui-实验性并行)。
后续 `0.2.x-rc` 桌面候选计划只发行 GPUI，成熟并完成迁移/回退验收后退役 Tauri 桌面链路；
当前保留可用的 Android 壳，不据此宣布生产切换或批准公开发布。路线与边界见
[ADR-0014](docs/adr/0014-gpui-native-integration-and-distribution.md)。

首个 `0.2.0-rc.1` 候选以 **Windows 非 TUN** 为验收范围：用户态核心、显式 HTTP/SOCKS 代理、
系统代理的有界接管与恢复，以及有限 Surge/Quantumult X 脚本能力。TUN 在启动准入处拒绝，
不改写已保存选项或触发提权；最终 UI、真实数据迁移及 TUN 后置。候选通过本地验收与签名校验
不等于已公开发布或获得其他平台运行证明。脚本支持边界见[开发指南](docs/development.md)。

本仓可独立构建，不需要 ProxyPanel 检出，不作为其 submodule。
[文档索引](docs/index.md) · [代理指南](AGENTS.md) · [来源记录](docs/source-provenance.json)。

## 许可与来源

AGPL-3.0-or-later，保留 ProxyPanel 与 Lanpath 贡献者归属。源码由 ProxyPanel 客户端整理而来；
服务端历史保留于原仓，来源基线与工作区变更记录见来源记录。
sing-box、wintun 等第三方组件的许可资产继续随分发保留。
