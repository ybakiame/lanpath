# Lanpath NSH Desktop

`apps/desktop` 是当前 NSH Windows 桌面实验目标（Rust、NSH 0.6.0），
程序名 `lanpath-desktop-nsh.exe`，应用身份 `com.ybakiame.lanpath.desktop.nsh`。
与 GPUI 及历史客户端的数据独立，不读取或迁移它们的数据。

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
旧系统 NSH 偏好不会自动导入。无显式覆盖时标记非法，或选定目录不可写时，启动失败，不回退其他目录。

每个数据根独立持有进程锁，同一根不支持同时打开多个实例；不同根可分别验证。
核心端口仍需避免冲突。非空目录必须已有本应用归属，生产与其他应用数据根、
符号链接/junction 和共享硬链接继续拒绝。Headless/嵌入测试仍可使用
`lanpath_desktop_nsh.data_dir` storage 键注入；原生程序使用启动时固定的数据根。

## 当前功能

Windows 使用带内嵌应用图标的自定义标题栏；窗口按钮、双击最大化、拖动与贴靠沿用系统行为。磁盘日志以只读编辑器展示，可滚动、选择和复制正文。

- 概览、订阅与核心：添加/刷新订阅，选择当前订阅，双核心库存与真实配置校验、启停。
- 规则：六类常用匹配、快速表单、批量简写、筛选、编辑、启停、排序和删除；重启核心生效。
- 诊断：系统 DNS、直连 TCP、已保存 DNS 上游、mixed/Clash 检查及本地规则预估。
- 覆写：本地/远程 TypeScript，远程先于本地；不执行 YAML 覆写，不提供模块/文件/网络能力。
- 请求设置、配置预览、日志、加密备份与恢复、主题/语言及减少动效。

核心仅运行用户态 loopback mixed 和 Clash API；Windows 系统代理经独立普通权限
守护接管/恢复。没有 TUN/VPN 或 MITM，未完成生产切换与发布。部分既有页面
尚未完成英文翻译，完整键盘与屏幕阅读器体验仍在完善。

开发方案、设计稿、验收记录及历史工作日志统一放在本地 `.work/`。
