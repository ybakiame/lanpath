# 岚径 · Lanpath

跨平台代理客户端，提供订阅管理、核心启停、配置切片、DNS 测速、规则集与分流规则、网络诊断、日志排查与加密备份。

当前桌面版采用纯 Rust 原生框架 **NSH 0.6.0**（位于 `apps/desktop`，当前候选版本 `0.2.1-rc.3`），不依赖 WebView；共享 Rust 核心业务位于 `crates/lanpath-*`，公共图标与资源位于 `assets/`。

## 桌面端主要能力

- **概览、策略与连接**：首次准备引导、WGSL Shader 氛围主卡与原生 Plot/Canvas 流量与 7 日统计图表、订阅管理抽屉、配置预览、核心（sing-box / Mihomo）下载校验与启停、运行态策略组/节点测速切换、实时连接追踪与一键写入规则，以及 Windows 普通权限系统代理接管/恢复。
- **sing-box 配置切片**：按 `log` / `dns` / `route` / `outbounds` / `experimental` 分段管理，支持可视化表单与 JSON 源码双模式编辑、国内外推荐 DNS 一键添加/替换与延迟探测、地区出站分组预设与 `$nodes` 正则筛选展开。
- **规则集与分组**：受管规则集清单与社区市场、条件 HTTP 更新、按引用本地注入；支持在「规则集」与「规则」页共用抽屉管理 `@分组`，并通过 Popover 弹窗按 Tab 检索与去重选择规则集或分组。
- **规则、脚本与诊断**：支持域名/后缀/关键词/CIDR/进程/端口/`ruleset` 规则编辑与排序、本地 TypeScript 覆写编辑器、系统与上游 DNS/端口诊断及本地规则预估。
- **外观、日志与备份**：OKLCH 配色体系（`lanpath` 默认品牌主题、`ocean`、`teal` 及 4 色板自定义 `ColorPicker`）、深浅色/跟随系统、Windows 11 Acrylic / Mica 窗口材质与透明度调节、内存与磁盘只读日志查看，以及 age 加密备份导出与事务恢复。

桌面版使用与便携模式说明见 [桌面说明](apps/desktop/README.md)。

## 便携模式

在程序同级存在且可写入的 `data/` 文件夹时自动启用便携模式：所有配置、订阅、规则、规则集、核心运行文件、日志与界面偏好均保存在同级 `data/` 目录中，退出后可整体移动目录，不依赖终端或快捷方式工作目录。

## 本地构建

Linux 统一通过仓库固定版本 standalone devenv（Nix）构建，环境配置见 [环境指南](devenv.md)；Windows 桌面版使用原生 Rust / MSVC / CMake 工具链。

```bash
# Linux
devenv shell -- cargo run --locked --manifest-path apps/desktop/Cargo.toml

# Windows (PowerShell)
cargo build --locked --manifest-path apps/desktop/Cargo.toml
.\apps\desktop\target\debug\lanpath.exe
```

提交检查脚本工具依赖位于仓库根目录，使用 `devenv shell -- bun install --frozen-lockfile` 安装。

## 移动端基础库

可复用的 Android 原生库与 Go/libbox 绑定位于 [mobile/](mobile/README.md)。当前仅提供原生库与构建入口，暂未提供可安装 APK。

```bash
devenv shell -- lanpath-build-aar
devenv shell -- lanpath-build-android-debug
```

## 文档与版本

- [桌面说明](apps/desktop/README.md) · [文档索引](docs/README.md) · [更新日志](CHANGELOG.md)
- 版本规范见 [版本约定](VERSIONING.md)，使用根 `VERSION` 与 `scripts/version.py` 统一同步桌面应用与共享 Rust crates。

## 许可与来源

AGPL-3.0-or-later，保留 ProxyPanel 与 Lanpath 贡献者归属。sing-box、wintun 等第三方组件许可随分发保留。
