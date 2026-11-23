# Lanpath NSH Desktop

`apps/desktop` 是 Lanpath 的纯 Rust 原生桌面应用（NSH 0.6.0，当前候选版本 `0.2.1-rc.2`），程序名为 `lanpath-desktop-nsh.exe`，应用标识为 `com.ybakiame.lanpath.desktop.nsh`。使用全新独立数据目录，不读取或迁移旧版客户端数据。

Lanpath 帮助你将可信订阅转化为可用的本地代理：准备订阅与核心，经校验后启动，并独立查看核心运行状态与系统代理接管状态；排查问题时可结合策略、连接监控、规则、配置切片、诊断与日志快速定位。

产品与界面规范见 [循径设计系统](../DESIGN.md)。新数据根默认使用 `lanpath` 主题，明暗模式与语言默认跟随系统。

## 运行

Linux 从仓库根进入 `devenv shell` 后运行：

```sh
cargo run --locked --manifest-path apps/desktop/Cargo.toml
```

Windows 使用原生 Rust / MSVC / CMake：

```powershell
cargo build --locked --manifest-path apps/desktop/Cargo.toml
.\apps\desktop\target\debug\lanpath-desktop-nsh.exe
```

## 便携模式与数据目录

在程序同级创建空文件 `lanpath.portable`，下次启动即使用同级 `data/`：

```text
Lanpath/
├── lanpath-desktop-nsh.exe
├── lanpath.portable
└── data/
    ├── app.toml                  # 应用设置、本地入口/Clash API 与 [ui] 外观偏好
    ├── rules.toml                # 自定义分流规则
    ├── overwrite/sing-box/       # 分段配置切片 (log/dns/route/outbounds/experimental.json)
    ├── rulesets/                 # 受管规则集清单 manifest.json 与 files/ 缓存
    ├── runtime/
    │   ├── db/                   # 运行时 SQLite 数据库（stats.db / cache.db，不纳入备份）
    │   └── <core>/               # 运行时合成配置 config.{json,yaml} 与 <core>.pid
    └── .cache/                   # 可重建缓存（订阅缓存、NSH 界面缓存等）
```

路径解析始终以程序实际所在目录为准，不受终端或快捷方式工作目录影响；退出应用后可整体移动便携目录。

如需指定独立目录，可通过 `LANPATH_DESKTOP_DATA_DIR` 环境变量覆盖（优先级高于便携标记）：

```powershell
$env:LANPATH_DESKTOP_DATA_DIR = 'D:\LanpathData'
.\lanpath-desktop-nsh.exe
```

未设置标记与覆盖时，Windows 默认使用 `%LOCALAPPDATA%\lanpath-desktop-nsh`，Linux 默认使用 `$XDG_DATA_HOME/lanpath-desktop-nsh`（或 `~/.local/share/lanpath-desktop-nsh`）。每个数据根持有独立进程锁，同一目录不支持多开；不同目录可并行隔离使用。

## 功能概览

- **窗口与外观**：Windows 自定义标题栏与内嵌图标，默认 `1180 × 800 pt`、最小 `960 × 640 pt`；侧栏可收起与展开并保留页面草稿；采用 OKLCH 配色体系（`lanpath` 品牌默认、`ocean`、`teal` 与 4 色板自定义 `ColorPicker`）、深浅色/跟随系统、减少动效，以及 Windows 11 Acrylic / Mica / 不透明窗口材质与透明度滑块。
- **概览、策略、连接与核心**：首次准备引导与原生 Shader/Plot/Canvas 实时流量和 7 日统计可视化；概览页提供右侧订阅抽屉、配置预览抽屉与覆写脚本抽屉；支持运行态策略组/节点测速热切换与实时连接追踪（支持一键加规则）；核心页支持 sing-box / Mihomo 下载、版本管理与校验。
- **配置切片（sing-box）**：覆盖 `log`、`dns`、`route`、`outbounds`、`experimental` 五段切片，支持可视化表单与 JSON 源码双模式编辑、实时校验、格式化与单切片重置；DNS 切片内置国内外推荐 DNS 预设与实时测速；出站分组支持地区快捷预设与 `$nodes` 正则筛选展开；路由与 DNS 规则集选择均使用 Popover 弹窗（Tab 分离、检索、已配置去重）。
- **规则集与分组**：受管规则集支持远程 URL（ETag / SHA-256 条件更新）与手动 JSON 规则集，并提供精选社区市场（含按需自选的 `geosite-private` / `geoip-private` 等，不强制内置）；支持将多个规则集聚合成应用内 `@分组`，可从「规则集」页与「规则」页随时呼出共用右侧抽屉进行管理，并在配置合成时自动展开、按引用注入本地规则集。
- **规则与诊断**：支持 `domain`、`suffix`、`keyword`、`cidr`、`process`、`port`、`ruleset` 七类匹配，提供快速表单（含 `@分组`/规则集快捷点选）、批量简写、筛选、启停与排序；诊断页支持系统 DNS、直连 TCP、已保存 DNS 上游、本地端口探测与本地规则预估。
- **覆写脚本**：本地 TypeScript 编辑器，带语法高亮、行号、撤销、括号缩进与常用字段补全，新建以 `function main(config) { return config; }` 起步。
- **请求设置、日志与备份**：支持独立代理策略（直连/系统/核心/自定义代理）；内存日志与磁盘只读日志分栏查看与导出；支持 age 加密备份（自动生成/复制强密码）与导入前摘要预览、事务化重启恢复。
- **草稿与并发保护**：未保存草稿受到离页与关闭保护，备份加解密/恢复等耗时事务进行中阻止误离页。

## 运行范围说明

- 「核心运行中」仅表示核心进程正常运行，不代表目标网络必然可达；系统代理保存偏好与实际接管状态分开展示。
- 订阅、配置切片、规则与覆写脚本保存后，会在下次启动或重启核心时生效；诊断中的本地规则预估仅供排查参考。
- 当前版本仅运行用户态 loopback mixed 入口与 Clash API，并通过独立普通权限守护接管/恢复 Windows 系统代理；不包含 TUN/VPN 或 MITM 功能。

## 版本与开发调试

- 产品版本统一由仓库根 `VERSION` 控制，详见 [版本约定](../../VERSIONING.md)；当前处于 `0.2.x-rc.N` 候选阶段（`0.2.1-rc.2`），不提供旧数据兼容或迁移。
- 开发者可通过 `--ui-gallery` 参数启动组件画廊：`cargo run --locked --manifest-path apps/desktop/Cargo.toml -- --ui-gallery`（普通导航不暴露该入口）。
