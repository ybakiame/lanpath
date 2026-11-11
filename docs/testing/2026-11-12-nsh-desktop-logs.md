# NSH Desktop 日志页切片验证（2026-11-12）

## 范围

`apps/desktop` 第三个切片：从 apps/gpui 迁移真实“日志”能力（`log_buffer`、
`disk_log_writer`、`log_capture`、runtime 日志服务与 logs_view/disk_logs 视图），
适配 NSH 桌面独立数据根与纯 Rust NSH 组件。apps/gpui 仅作迁移来源、未修改；
apps/nsh 不动；仍不启动核心、系统代理、TUN 或 VPN，不读取其他应用日志。

- 内存分区：从 GPUI `log_buffer` 迁移有界缓冲（1000 条、单条 8192 字节按字符
  截断并标记 `[截断]`、revision 增量快照、淘汰计数、清空）。GPUI 的核心日志流状态
  （WebSocket 订阅 sing-box）整体不迁移：NSH 桌面不启动核心，没有核心日志来源，
  页面也不显示核心连接状态。
- 采集：`log_capture` tracing 全局层（每进程安装一次，幂等）只过滤 `lanpath_*`
  target 的本应用事件（`lanpath_desktop_nsh`/`lanpath_client`/`lanpath_common`），
  扇出到所有注册服务；生产进程只有一个注册服务，headless 并行场景各持独立数据根
  互不影响数据文件（各自磁盘写入器）。消息字段与级别映射沿用 GPUI 语义。
- 磁盘写入：`disk_log_writer` 去掉 GPUI 的 core.log 通道，只落本应用
  `logs/app.log.YYYY-MM-DD`（日轮转、tracing-appender 非阻塞、buffered_lines_limit
  1024、I/O 与队列丢条真实上报）。WorkerGuard 由 `LogService` 持有，服务丢弃/
  进程退出时冲刷并停止写入线程（取消/退出清理）。
- 磁盘读取：复用共享 `lanpath-client::logs::DiskLogService`（文件名白名单、
  拒绝符号链接/junction/硬链接共享文件、O_NOFOLLOW 类打开、2 MiB / 1000 行尾部
  上限），只读本应用数据根的 `logs/`。
- 导出：GPUI 写 `<root>/exports/export-<stamp>.log`；NSH 桌面改用 NSH 原生保存
  对话框（`pick_save_path`）由用户选定目标，按文件名顺序拼接 app.log* 原文写出：
  `create_new` 不覆盖已有文件、逐文件快照长度、失败删除半成品。取消不做事；
  平台无对话框时保存对话框错误真实上屏。未完成/做不到的子功能不显示入口。
- 服务装配：`LogService` 按数据根路径进程内缓存（Weak），页面来回切换不重开缓冲；
  与订阅共用 `data_root::open_root` 同一条租约（第二进程仍被拒绝）。服务随应用
  启动开始采集，订阅新增/启停/删除成功后记录操作结果，不记录可能含凭据的订阅
  地址。磁盘写入器创建失败降级为纯内存并持久显示真实错误；磁盘分区仍明确报告
  不可用，导出失败原样上屏，不伪造成功。
- UI 全部为 NSH 原生 button/dialog/signal/each/scroll + `set_interval` 轮询
  （句柄随 owner 存活、卸载即取消）+ `spawn` 导出任务（Stored 持有、可替换），
  无 TSX 产品 UI。

## 验证

```sh
devenv shell -- cargo fmt --manifest-path apps/desktop/Cargo.toml -- --check
devenv shell -- cargo test --manifest-path apps/desktop/Cargo.toml --locked
devenv shell -- cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings
devenv shell -- cargo check --manifest-path apps/desktop/Cargo.toml --locked
devenv shell -- cargo build --manifest-path apps/desktop/Cargo.toml --locked
```

- fmt：通过，无 diff。
- `cargo test --locked`：59 项通过、1 项 ignored（跨进程锁争夺子进程用例，由父用例
  以子进程驱动并通过）。其中 `src/` 37 项（数据根准入 20、订阅 5、日志缓冲 3、
  采集 2、磁盘写入 2、日志服务 5），`tests/` headless 22 项（日志页 12 新增、
  订阅 7、外观与导航 3）。日志页用例连跑 3 轮稳定通过。
- headless 覆盖（`nsh_testing::Headless`，每场景独立临时数据根 + 桌面视口）：
  内存分区就绪态；tracing 事件真实采集上屏；级别筛选四档过滤；未暂停时轮询
  自动带出（`wait_for_within` 5s）；暂停后超过一个轮询周期新事件不上屏、手动
  刷新仍可用；行选中详情；清空需确认且确认后消失；磁盘分区真实文件列表与
  尾部原文（后台写入线程落盘经短轮询等待）；事件真实落数据根 `logs/`；
  导出经 `Headless::on_pick_save_path` 注入保存对话框应答、断言导出文件真实
  内容与格式；分区切换后内存缓冲保留。
- clippy `--all-targets -D warnings`、`cargo check --locked`、`cargo build --locked`：
  通过。

## Windows 原生验收

在 Windows 本地副本使用 Rust 1.98.0 / MSVC 执行 `cargo build --locked` 成功。以独立临时
数据根启动真实 `lanpath-desktop-nsh.exe`，通过 Windows UI Automation 和系统原生窗口
完成以下闭环，未读取或修改 Tauri/GPUI 数据：

- 新增“日志验收订阅”后，日志服务从应用启动开始采集；进入内存分区即自动显示真实
  INFO 事件，记录操作名称，不含订阅 URL。
- “错误”筛选显示 0 条，恢复“信息”重新显示；选择行展示完整详情；“暂停/继续”
  切换生效。清空对话框选择“取消”保留记录，选择“确认清空”后缓冲归零。
- 磁盘分区列出真实 `app.log.*` 文件并显示同一事件原文。清空内存后磁盘原文仍在。
- NSH 调出 Windows 系统保存对话框，选定临时目标后成功导出；实际文件与磁盘日志
  原文一致，窗口显示保存路径。自动化输入路径和对话框控件定位问题属于测试脚本调整，
  未改动产品行为。
- 通过 `WM_CLOSE` 正常退出，进程在 10 秒等待内结束。

## 环境备注

仓库 rust-toolchain 固定 1.98.0；宿主 /usr/sbin 的 rustup 代理在 devenv 之外会把
proc-macro 链接到宿主 glibc 2.44、运行时经 Nix profile 加载 Nix glibc 2.42，表现为
`rquickjs-macro` E0463（与本次改动无关、HEAD 即复现）。验证一律在
`devenv shell --` 内执行（Nix rustup + 一致 glibc），不受影响。

## 边界

内存日志仅限本次进程：应用重启即空，磁盘文件保留；清空不删除磁盘文件、不停止
采集。磁盘导出为明文原文拼接，未加密。磁盘分区只读本应用数据根内白名单日志文件，
不提供其他目录选择（GPUI 亦只读实验目录）。核心日志流（sing-box WebSocket）随
“不启动核心”边界整体未迁移，后续接管核心时须单独切片。Windows 原生窗口与
保存对话框已按上节验收；Linux headless 的保存对话框为注入应答，平台对话框 UI
本身不在 headless 断言范围。
