# Lanpath NSH Desktop

`apps/desktop` 是当前 NSH Windows 桌面实验目标（Rust、NSH 0.6.0），
程序名 `lanpath-desktop-nsh.exe`，应用身份 `com.ybakiame.lanpath.desktop.nsh`。
与 GPUI 及历史客户端的数据独立，不读取或迁移它们的数据。

Lanpath 帮助你将可信订阅变成可用的本地代理：准备订阅和核心，校验后启动，
分别确认核心运行与系统代理接管状态。遇到问题时，可继续查看诊断、规则和日志。

共用的产品与界面规范见 [循径设计系统](../DESIGN.md)，涵盖桌面与后续 Android
的统一视觉语言和平台适配。Windows 已接入图标导航、配置路径概览、首次使用引导
和统一深浅色表面；新数据根默认 Ocean，默认跟随系统明暗与语言。Android 仍为后续适配目标。

## 运行

Linux 从仓库根进入 `devenv shell`，再执行：

```sh
cargo run --locked --manifest-path apps/desktop/Cargo.toml
```

Windows 使用原生 Rust/MSVC/CMake：

```powershell
cargo build --locked --manifest-path apps/desktop/Cargo.toml
.\apps\desktop\target\debug\lanpath-desktop-nsh.exe
```

## 便携模式与隔离数据

在程序同级创建空文件 `lanpath.portable`，下次启动使用同级 `data/`：

```text
验证目录/
├── lanpath-desktop-nsh.exe
├── lanpath.portable
└── data/
```

以程序实际所在目录为准，终端或快捷方式的工作目录不参与解析；`.portable`
不生效。业务配置、订阅、规则、核心库存、日志，以及 NSH 主题/语言偏好和
图片缓存都放在此 `data/` 下。可给不同验证目录各放一份程序与标记文件。
退出应用后可整体移动便携目录，已有归属、规则与偏好保留。

`LANPATH_DESKTOP_DATA_DIR` 环境变量优先于便携标记，可显式选择一个独立目录：

```powershell
$env:LANPATH_DESKTOP_DATA_DIR = 'D:\LanpathTests\case-a'
.\lanpath-desktop-nsh.exe
```

没有标记或覆盖时，Windows 使用 `%LOCALAPPDATA%\lanpath-desktop-nsh`；
Linux 使用 `$XDG_DATA_HOME/lanpath-desktop-nsh` 或
`~/.local/share/lanpath-desktop-nsh`。正常退出后添加/移除标记再启动即可切换，
不会自动搬迁、合并或删除已有数据。切换后主题等偏好也按数据目录隔离，
外观偏好统一保存在 `app.toml` 的 `[ui]` 节（主题、明暗、语言、减少动效），NSH 运行状态置于 `[ui.runtime]`，缓存置于 `cache/nsh/`。不再生成 `storage.json` 或 `default/`。无显式覆盖时标记非法，或选定目录不可写时，启动失败，不回退其他目录。

每个数据根独立持有进程锁，同一根不支持同时打开多个实例；不同根可分别验证。
核心端口仍需避免冲突。非空目录必须已有本应用归属，生产与其他应用数据根、
符号链接/junction 和共享硬链接继续拒绝。Headless/嵌入测试仍可使用
`lanpath_desktop_nsh.data_dir` storage 键注入；原生程序使用启动时固定的数据根。

## 当前功能

Windows 使用带内嵌应用图标的自定义标题栏；窗口按钮、双击最大化、拖动与贴靠沿用系统行为。磁盘日志以只读编辑器展示，可滚动、选择和复制正文。

窗口默认 1180 × 800 pt、最小 960 × 640 pt，统一配置在 `nsh.json`。内容区标题左侧可收起/恢复菜单，当前页面与未保存草稿会保留。外观页提供紧凑的主题、明暗、语言与减少动效选项；主题选择只强调预览边框；Windows 支持 Acrylic、Mica 与不透明材质，并可调节表面不透明度。

- 概览、440 pt 订阅 Sheet 与核心：首次准备引导，区分当前配置与运行会话；Enter 添加订阅、按名称筛选长列表，订阅与核心库存按可用空间排列为一列或两列卡片，支持真实配置校验、启停。
- 规则：六类常用匹配、快速表单、批量简写、筛选、编辑、启停、排序和删除确认；重启核心生效。
- 诊断：系统 DNS、直连 TCP、已保存 DNS 上游、mixed/Clash 检查及本地规则预估。
- 覆写脚本：从首页 Sheet 管理，使用原生 Dialog 与 TypeScript Editor 编辑本地脚本；内置高亮、行号、撤销、括号/缩进和本地字段提示。新建以 `function main(config) { return config; }` 为模板；不提供远程 URL 输入。补全不包含完整类型检查。
- 配置预览从概览打开：选择一个启用订阅后可查看保存原文；关联覆写脚本时才生成最终配置。预览不代表正在运行的核心快照。
- 请求设置、日志、加密备份与恢复；日志行和磁盘文件列表左对齐，长文件名裁切；备份密码可生成并显式复制，导出成功后清空输入。
- 订阅、规则、覆写、请求设置和备份的未保存草稿受到离页/关闭保护；导出、加解密与恢复事务进行中需等待完成后再离开。
- 主题/语言及减少动效；侧栏选中滑动、按钮按压与真实等待反馈遵循动效偏好。

“核心运行中”表示核心运行事实，不保证互联网可达；系统代理的保存偏好与实际归属
分别显示。订阅、规则与覆写更改保存后，需在下次启动或重启核心时应用。
诊断中的本地规则预估不代表运行核心的实际路由。

核心仅运行用户态 loopback mixed 和 Clash API；Windows 系统代理经独立普通权限
守护接管/恢复。没有 TUN/VPN 或 MITM，未完成生产切换与发布。部分既有页面
尚未完成英文翻译，完整键盘与屏幕阅读器体验仍在完善。

开发方案、设计稿、独立交互画廊与验收记录统一放在仓库 `.tmp/work/`；
共用长期规范维护在 `apps/DESIGN.md`。现有原生组件画廊可通过
`cargo run --locked --manifest-path apps/desktop/Cargo.toml -- --ui-gallery` 打开
（Linux 先从仓库根进入 `devenv shell`）。画廊仅用于组件检查，普通导航没有此入口。

产品版本统一取自根 `VERSION`，版本递增与检查见 [版本约定](../../VERSIONING.md)。Windows 标题栏和文件详情展示完整候选版本；当前处于 `0.1.x-rc.N`，使用新数据根，不提供旧数据兼容或迁移。
