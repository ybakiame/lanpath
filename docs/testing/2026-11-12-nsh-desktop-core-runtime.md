# NSH Desktop 核心二进制校验与用户态启停切片验证（2026-11-12）

## 范围

`apps/desktop` 第八个切片：核心运行时（`src/core_runtime.rs`）——真实核心
二进制校验 + 用户态启停。从共享 `crates/lanpath-engine` 复用
`SingBoxProcessManager`/`MihomoProcessManager`（pid 身份记录、Windows Job、
Unix 生命周期绑定），交互参考 `apps/gpui/src/runtime`（未改动 GPUI 产品
代码）。apps/nsh 不动；不接管系统代理/TUN，不操作生产进程，不做流量/连接
页面。核心以 sing-box 为主，Mihomo 只需能运行订阅配置。

**Windows 真实核心验收待主代理执行**（本切片的自动化测试全部使用 rustc
现场编译的 fixture 假核心，不是真实 sing-box/Mihomo，见下文“测试边界”）。

## 端口（breaking change，仅新根）

新数据根默认端口改为 GPUI 实验值：**mixed_port=17899、clash_api_port=19090**
（`src/core_runtime.rs` 的 `DEFAULT_MIXED_PORT`/`DEFAULT_CLASH_API_PORT`，
由 `storage_init` 写入新根 `app.toml`），避免与本机生产实例的
17890/9090 冲突。已有统一 TOML 根保留已保存值，不重复改写。

## CoreRuntime（`src/core_runtime.rs`）

按数据根共享：Weak 缓存 + App 启动时 `stored` 持有（`src/lib.rs`）——
同一数据根的所有页面/句柄看到同一份真实运行状态。GUI 正常退出 drop 时
按记录的 OS 进程身份（`ProcessIdentity::terminate_verified`）回收本次
子进程，绝不按进程名强杀；spawn 时的 OS 父子绑定（Windows Job
KILL_ON_JOB_CLOSE / Linux PDEATHSIG）是兜底。

- 内部 `ActiveManager` enum 复用两个引擎 manager，lifecycle `tokio::Mutex`
  持有；启动/停止实现整体在 `tokio::spawn` 独立任务中跑到完成——UI 离页
  取消只是看不到结果，不会留下半启动子进程。
- 启动固定同一份 `ClientConfig` 快照（业务锁内读出）：拒绝
  `tun_enabled`/`system_proxy_enabled`（本切片无授权，设置保留）；
  必须有 confined 库存选中核心并经 `validate_managed_binary`；先预检
  mixed/Clash API 端口 loopback 可 bind（共享 `port_guard`，错误含端口与
  占用者）；`ConfigComposer::compose_with_config(快照)` 一次合成（订阅
  刷新、覆写、规则资源同一快照，不重复联网；`compose` 仍先读已保存快照
  的便捷入口）；Mihomo YAML 经 serde_yaml 解析回 `Value` 供引擎。
- `ConfigValidator` 校验通过后才 `manager.start`；再等待 mixed 端口真实
  监听（核心中途退出立即报错并带核心输出尾行）、Clash API 启用时
  `wait_clash_api_ready`。任何失败回滚 `stop_process`；回滚失败同时报告
  原错误 + 回收错误，manager 保留在运行时里（再次启动/停止先重试回收，
  可重试状态）。
- `status()` 每次调用都真实 `is_running` poll：核心异常退出在此被发现、
  记录（“核心进程异常退出”）并清理会话，不只记按钮状态；下次成功启动或
  手动停止时清除记录。

## ConfigValidator（`src/config_validator.rs`，独立文件）

真实核心 schema 校验：sing-box `check -c <临时文件> -D <根>`、Mihomo
`-t -d <根> -f <临时文件>`；临时文件建在本数据根内，`no_window` 抑制
Windows 控制台窗口，stdout/stderr 并行 drain（各保留 ≤32 KiB 诊断），
20 秒超时强杀并回收（测试可注入更短超时）。

能力边界（不搬 GPUI 全量字段白名单，只守本切片安全语义）：

- sing-box：有且仅有一个 `mixed` inbound，`listen==127.0.0.1`、
  `listen_port` 等于已保存 mixed 端口；拒绝 tun/`set_system_proxy: true`/
  非 loopback/多 inbound；`experimental.clash_api` 启用时必须匹配已保存
  loopback 端口与密钥、禁用时必须缺席、不得携带 `external_ui`/
  `external_ui_download_url`；`log.output` 写路径限根内 `logs/`、
  `experimental.cache_file.path` 限根内 `cache/`、`route.rule_set` 本地
  资源与 `certificate.certificate_path` 读路径限根内（拒绝 `..`/`$`/
  链接/junction/硬链接/Windows 设备名）；输出路径不得覆盖选中的核心
  可执行文件。
- Mihomo：`mixed-port` 匹配；`allow-lan` 只允许 false/缺席；`tun.enable`
  只允许 false/缺席；`external-controller` 启用时必须匹配已保存 loopback
  端口、禁用时必须缺席；顶层 `external-ui`/`external-ui-url` 必须不存在。

## 启停与库存互斥（`src/core_inventory.rs`）

启动/停止持有 `CoreInventory` 同根 mutation 锁：start 途中 select/
download/delete 立即 busy（同一 `MUTATION_BUSY` 文案）；`select`/
`download`/`delete` 在锁后真实检查 `CoreRuntime::is_running_sync`
（`ProcessIdentity::capture` 核验 pid+创建时间+镜像，不看 UI 状态），
核心运行中（含停止进行中 running 未清）拒绝修改核心文件或选择。取锁
顺序恒为 mutation → lifecycle → business/active（短临界区），无反向
路径，不会死锁。

## UI（`src/core_view.rs` + `core_view/runtime.rs` + `core_view/runtime_card.rs`）

核心页新增“运行”卡片（位于“当前使用”与“库存”之间）：

- “校验并启动”：文案明确“先以真实核心二进制校验配置（sing-box check /
  Mihomo -t），再以用户态 loopback mixed + Clash API 启动 · 不接管系统
  代理/TUN”；结果文案明确“已完成二进制校验并启动”（类型/版本/端口/
  节点数/警告数，不含配置正文）。
- “停止”：回执展示真实停止 kind（强制终止并回收 / 早已退出）。
- 真实 running/busy/结果/停止 kind/错误上屏；状态轮询（500ms）能发现
  核心异常退出；运行状态经按根共享的 CoreRuntime 跨页一致（切页返回仍
  显示运行中）。库存管理（查询/下载/选择/删除）原样保留。
- 配置预览页不变：继续单独合成未校验预览，不显示“已验证/已启动”。

## 日志

只记操作、端口、版本、阶段与停止 kind（tracing target
`lanpath_desktop_nsh::core_runtime`/`core_inventory`）；不记配置正文、
节点/代理凭据、完整 URL。

## 测试边界（重要）

- 启动/停止/校验用例全部使用 **rustc 现场编译的 fixture 假核心**
  （`tests/fixtures/core_fixture.rs`，真实进程身份、真实监听、真实退出
  码、行为开关经配置文本或 `<binary>.behavior` sidecar 驱动）——它不是
  真实 sing-box/Mihomo，**不得据此宣称 Windows 真实核心验收通过**。
- headless 用例驱动真实按钮与真实运行时（fixture 进程），不 mock 成功；
  仅覆盖按钮状态/文案/跨页共享/异常退出上屏。Windows 真实 sing-box/
  Mihomo 二进制验收（下载、校验、启停、退出无残留）由主代理执行。
- 业务逻辑测试不依赖公网：订阅走本地缓存（Core 请求策略），规则集无
  引用；fixture 编译只链接 Cargo 已构建的 serde_json rlib（沿用
  crates/lanpath-client state tests 的同一模式）。
- pid 身份读取失败的故障注入不使用测试线程竞争删文件（不可靠），由
  fixture 在引擎写记录后按确定延时删除 + 延迟 API 绑定撑开读回窗口，
  时序 margins ≥700ms，并行负载下确定。

## 验证

```sh
devenv shell -- bash -lc 'cargo fmt --manifest-path apps/desktop/Cargo.toml && \
  cargo test --manifest-path apps/desktop/Cargo.toml --locked && \
  cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings && \
  cargo build --manifest-path apps/desktop/Cargo.toml --locked'
```

（`--locked` 前需以 `--offline` 更新一次 `apps/desktop/Cargo.lock`：新增
`lanpath-engine` 直接依赖、`serde` derive、`tempfile` 移入正式依赖、tokio
增加 `net`/`process`/`io-util` feature。）

结果（2026-11-13，`nsh-validation` 工作区）：

- `cargo fmt -- --check`：通过，无 diff（根 `cargo fmt --all -- --check` 也通过）。
- `cargo clippy --locked --all-targets -- -D warnings`：通过，零警告。
- `cargo build --locked`：通过。
- `cargo test --locked`：**194 项通过、1 项 ignored**（存量 157+1 全保留），
  其中 `src/` 128 项（运行时 14、校验器 17、合成快照 1、存储初始化端口
  断言、存量 96 项），`tests/` headless 66 项（核心页 13，含运行卡片 5 项
  新增；其余页 61 项存量）。core_page 二进制在全量并行下约 137 秒（fixture
  现场 rustc 编译 + 轮询等待在 8 个测试二进制并行下的正常排队；单独运行
  约 5 秒），无失败无重试。
- `git diff --check`：通过；`scripts/check-file-size.sh`：通过（仅预警线警告，
  无超线）。测试结束后无残留 fixture 核心进程、无测试端口占用。

覆盖明细（在 157 项存量基础上新增 35 项）：

- 运行时 14 项（`src/core_runtime_tests.rs`）：happy path（校验→真实
  进程→真实 mixed 监听→API 就绪→停止 Forced→端口释放）、未选核心、
  拒绝 TUN/系统代理、端口占用预检（不 spawn）、二进制校验失败（含核心
  诊断、不 spawn）、监听超时回滚并可重试、**启动阶段失败走同一 rollback**
  （fixture 预置损坏 pid 记录 → 引擎 `identity record failed` + 本层
  stop_process 经 reap_stale 读到损坏记录 → 合并报错“原错误 + 启动回收
  未完成 + invalid recovery identity”，manager 保留可重试状态；清理故障后
  停止重试成功、再次启动成功）、**就绪后 pid 身份读取失败走同一 rollback**
  （fixture 在引擎写记录后删除 pid 文件 + 延迟 API 绑定撑开窗口 → 读身份
  失败 → 回滚真实杀掉存活子进程，端口立即释放而非 detach 到进程退出）、
  异常退出被状态 poll 发现并 可再启动、运行中库存 select/download/delete
  拒绝+停止后恢复、启停与在途 mutation busy、空闲停止 AlreadyStopped、
  drop 回收子进程、Mihomo 订阅配置 happy path。
- 校验器 17 项（`src/config_validator_tests.rs`）：sing-box/Mihomo 能力
  边界正/负例（tun/多 inbound/非 loopback/错端口/set_system_proxy/
  clash_api 匹配/面板下载字段/日志与缓存与 rule_set 与 certificate 路径
  收敛/覆盖核心二进制拒绝/Windows 设备与 ADS 形态）+ fixture 真实
  check/-t 成功/失败/超时强杀。
- 合成快照 1 项：compose_with_config 用调用方快照而非磁盘值。
- 存储初始化：新根默认端口 17899/19090。
- headless 5 项（`tests/core_page.rs`）：运行卡片空态、校验并启动→真实
  监听→停止、二进制校验失败错误上屏、运行状态跨页共享、运行中库存
  选择拒绝+停止后恢复、异常退出轮询上屏。
- 端口断言更新：配置预览/合成测试从 17890/9090 改为 17899/19090。

## Windows 原生验收

使用 Windows Rust 1.98.0/MSVC 构建，以全新独立 TOML 数据根和真实 GitHub 官方
`sing-box v1.14.4` 完成验收。按用户提示避开主机现有 17899 核心，仅把本测试根
配置改为 mixed `17999`、Clash API `19190`；产品默认端口未因此改变。

- 下载官方核心，使用受控本地 HTTP 服务提供一个 Clash/SS 测试节点，设为生效。
- “校验并启动”实际完成 `sing-box check`，启动官方进程并监听 mixed/API；
  `GET 127.0.0.1:19190/version` 返回 200，pid 记录对应真实 sing-box PID。
- 正常停止显示真实停止回执，PID 文件删除，进程退出且 mixed 监听消失。
- 再次启动后仅强制结束本次记录 PID，页面报告“核心进程异常退出”并恢复未运行，
  监听消失，无残留进程；`WM_CLOSE` 正常关闭 GUI。
- 原生验收同时修复了数据根缓存键不规范化导致同进程 `\\?\` 别名被误报“目录
  正在使用中”的问题；初始化错误不再被 UI 吞掉，并新增 Windows 别名租约回归。

本轮 Windows 未下载或运行 Mihomo；Mihomo YAML 合成与 fixture 生命周期已覆盖，
真实 Mihomo `-t`/启停仍待后续验收。未接管系统代理/TUN，未修改现有 17899 核心。
