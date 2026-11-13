# NSH Desktop 系统代理守护与精确恢复切片验证（2026-11-13）

## 范围

`apps/desktop` 第九个切片：纯 Rust NSH 桌面的 Windows“系统代理守护与
精确恢复”。语义从以下来源迁移（复用语义而非简化，未改动 GPUI 产品
代码）：

- `apps/gpui/src/proxy_guardian/{mod,launch,protocol,rpc,security,service,windows,wire}.rs`
  → `apps/desktop/src/proxy_guardian/`（管道名改 `LanpathNSH-Proxy-` 独立身份）；
- `apps/gpui/src/privileged/ipc.rs` 的普通权限 `Pipe`（不带 TUN helper）
  → `apps/desktop/src/proxy_guardian/pipe.rs`；
- `apps/gpui/src/runtime/system_control.rs` 的 acquire/restore/recover/status
  → `apps/desktop/src/core_runtime/proxy.rs`；
- 共享 `crates/lanpath-client/src/sysproxy` 的
  `SystemProxySession`/`windows_with_owner`/DPAPI 日志/精确类型值恢复——
  本切片不复制、不修改共享会话；
- `apps/gpui/src/windows_gui.rs` 的 `ensure_unelevated`（改 NSH 文案）
  → `apps/desktop/src/windows_access.rs`。

本切片经任务授权接入系统代理（根 AGENTS 的 apps/desktop 边界由“不接
管系统代理”更新为“经独立守护接管系统代理”）；TUN 仍拒绝，不接管
VPN，不安装服务/启动项。APP_ID/owner_label 为
`com.ybakiame.lanpath.desktop.nsh`，owner 为本数据根 UUID
（`DataRoot::id`，即 `.lanpath-desktop-nsh-root-id` 的内容）。

**Windows 真实 WinINet 验收待主代理执行**：本切片全部自动化测试使用
fixture（fake spanner/共享 session 语义的内存实现），不是 Windows 真实
WinINet；不据此宣称已验证。

## 守护进程（`src/proxy_guardian/`）

- `main.rs` 在任何 NSH/GUI/存储/env_logger 初始化前分派
  `--proxy-guardian`（`dispatch_from_env`）：失败/拒绝非 0（6）退出且
  不留新 GUI；非 Windows 明确不支持（返回 6）。
- 同映像独立普通权限守护：CreateProcess 仅继承 GUI 进程查询句柄；
  CREATE_NO_WINDOW；宿主在 Job 内时 CREATE_BREAKAWAY_FROM_JOB，且
  守护启动后自验 `IsProcessInJob`（拒绝继承 Job，保证 GUI 死亡后可
  独立恢复）；与核心进程不在同一 Job（核心由引擎 manager 自己绑
  Job）。不安装服务/启动项。
- 参数有界版本化协议（`protocol.rs`）：恰 6 个参数（owner UUID、父
  pid、父创建身份、继承句柄、管道名），拒绝任意路径/附加命令；
  管道名必须是 `\\.\pipe\LanpathNSH-Proxy-<uuid>`。
- 身份核验全保留：命名管道 ACL（当前用户/BA/SY，拒绝远程客户端）、
  两侧 peer_pid 与绑定进程句柄、同映像校验（大小写不敏感）、
  SID/session/logon/elevation 一致且非提权（`security.rs` +
  `windows_access::ensure_unelevated`，NSH 文案）。
- 线协议（`wire.rs`）：`LPG1` 帧头 + 严格递增序列号 + 4096 字节上限；
  断帧/乱序/版本错不是回执。普通 `Acquire` 与显式接管授权
  `AcquireTakeover` 是形态可区分的两个请求（`PartialEq` 断言），伪造/
  重放的普通 Acquire 不能被当作接管授权。
- RPC（`rpc.rs`）：ready 帧 owner 匹配后才接受请求；显式拒绝保留
  流对齐，错形状/传输错误永久关流（守护侧完成操作并恢复）。
- 服务循环（`service.rs`）：守护（不是 GUI/RPC 调用方）持有全部变更
  与检查点；请求读失败与响应写失败进入同一条精确恢复路径
  （`session.restore()`）。`GuardianSession` trait 是守护内会话操作面，
  Windows 下由共享 `SystemProxySession` 实现（`acquire` 按策略直通
  共享 `acquire_with_policy`：普通请求恒 RequireDirect，仅
  `AcquireTakeover` 映射 ExplicitTakeover）；测试 fake 按同一契约
  建模（fixture，不是真实 WinINet）。

## CoreRuntime 集成（`src/core_runtime.rs` + `proxy.rs` + `status.rs`）

- 守护装配抽象（`src/system_proxy.rs`）：`GuardianSpawner`/
  `ArmedGuardian`。Windows 真实实现 arm 同映像守护进程；非 Windows
  显式不支持（`supported()==false`，运行时不因已保存偏好失败）。测试
  注入 fake spanner（共享 session 语义 fixture）。
- 启动：`system_proxy_enabled=true` 且平台支持时，核心校验/mixed/API
  全部就绪并记录会话后 arm 守护 + acquire `127.0.0.1:mixed_port`；
  **接管失败保留核心与守护证据**（不 rollback、不清证据）。启动前先
  回收上次残留管理器（stop_locked 含守护恢复），再
  `recover_proxy_before_start`：本实例仍持有（Owned）拒绝启动；
  abandoned journal 由独立守护完成恢复（GUI 死亡不能中断检查点）。
- 停止：`stop_locked` 先 `restore_proxy`（守护 shutdown=restore+退出
  确认；守护死亡/通信失败不是恢复证明，`has_recovery_for_owner` 为真
  才 arm 继任守护按 owner 精确恢复），再停核心。恢复失败保留核心与
  可重试证据（绝不跳过恢复直接停核心）。
- Drop：应用退出先尽力守护 shutdown，再按 OS 进程身份回收核心；恢复
  失败不阻断回收（独立守护在 GUI 句柄/管道断开时自行恢复，下次启动
  的 abandoned 恢复可重试）。
- 状态：`status()` 同一快照上报 `proxy_supported` + 守护真实
  ownership（未接管/本应用持有/需要恢复；查询失败如实上屏“未知错
  误”，不伪造成未接管）。
- **一次性显式接管授权（consent，响应真实验收发现）**：共享层正确拒绝
  foreign active manual proxy（RequireDirect），接管这类基线需要共享
  `AcquisitionPolicy::ExplicitTakeover` 且必须由用户显式同意——本切片
  不传不安全全局开关。`CoreRuntime.proxy_takeover_consent` 为进程内
  `AtomicBool`（默认 false、绝不持久化、重启即失效）；仅 UI 确认框
  确认后置位，停用即清除；启动 acquire 时读取并消费（成功或失败都只
  影响当次），消费后仅当次 acquire 以 `AcquireTakeover` 经
  `acquire_with_policy(..., ExplicitTakeover)` 发起；未确认时普通
  Acquire 仍 RequireDirect，foreign 仍被拒绝（无需按错误字符串做特殊
  UI）。
- 锁顺序：mutation → lifecycle → proxy_guardian → business/active
  （短临界区，business 只在 arm 守护前读取配置时持有）；守护 RPC 绝不
  持 business 锁；无反向路径。
- 非 Windows：启动忽略已保存偏好并如实记录日志；页面显示“仅 Windows
  支持”并隐藏接管操作；`--proxy-guardian` 直接非 0 退出。

## UI（核心页“系统代理”分区，`core_view/proxy_card.rs`）

- 开关只保存偏好（同一业务锁内读最新 `ClientConfig`，只改
  `system_proxy_enabled` 一个字段——配置合并，active 订阅/核心选择/
  端口/请求代理/MITM 等无关字段原样保留）。
- **首次启用弹一次性确认框**：文案明确“若已有其他代理，仅本次接管
  （显式授权），停止核心或退出应用时按原值精确归还，不覆盖外来代理；
  授权只生效一次、不保存，重启后需重新确认”。确认才保存偏好并授予
  进程内 consent；取消不写配置、不授权（开关保持关闭，再次按下重新
  弹框）。已保存 true 但新进程无 consent 时，用户停用再启用即可重新
  确认；禁用同时清除未消费的 consent。
- 运行中切换走安全启停周期（stop 含守护恢复 → start 按新偏好接管），
  失败如实上屏；不绕守护直写注册表。
- 状态行显示：未接管 / 本应用持有 / 需要恢复 / 未知错误。
- 显式“恢复系统代理”：无装配守护时先查 owner 精确记录，存在才 arm
  继任守护 `recover_watched`；外来/未知代理由共享会话拒绝覆盖，绝不
  无条件清空。
- 非 Windows：如实显示“仅 Windows 支持”说明；恢复操作隐藏，偏好
  开关仍可见可保存（本机不接管），确认框流程同样可交互。

## 测试

新增 35 项（fixture/fake 均非 Windows 真实 WinINet）：

- guardian 单测 16 项：`protocol` 3（创建身份非仅 PID、有界参数拒绝
  含 GPUI 管道前缀、跨用户/session/logon/提权拒绝）、`wire` 2（版本/
  序列/尺寸/截断帧、**普通 Acquire 形态上不是接管授权**）、`rpc` 4
  （迟到回执不可复用、显式拒绝对齐/错形状关流、ready 帧 owner 匹配、
  **AcquireTakeover 独立形态与回执/序列**）、`service` 7（顺序服务、
  拒绝对齐、**IPC 断开后恢复**、乱序不是回执、RecoverWatched 只认认证
  owner、恢复路径区分、**策略映射：普通恒 RequireDirect、仅
  AcquireTakeover 为 ExplicitTakeover**）。
- 偏好持久化 4 项（`system_proxy_tests.rs`）：往返、**配置合并**保留
  无关字段、损坏配置不被覆盖、非 Windows 不支持装配器显式安全。
- 运行时编排 13 项（fake spanner，`core_runtime/proxy_tests.rs`）：就绪
  后 arm+acquire 顺序、acquire 失败保留核心与证据、启动前 abandoned
  恢复顺序、恢复拒绝可重试、停止恢复失败保留核心可重试、Drop 先守护
  恢复、查询错误如实上报、显式恢复只走 owner 精确路径；**consent 5 项：
  未确认 foreign 仍被普通 Acquire 拒绝（绝不升级为 takeover）、确认
  一次只影响一次、失败也消费授权、停用清授权、授权不持久化（app.toml
  字节不变）**。存量 TUN/系统代理拒绝用例
  更新为“TUN 拒绝 + 非 Windows 偏好如实忽略不失败”。
- headless（`tests/core_page.rs`）：核心页文案与系统代理分区平台分支
  断言（非 Windows 显示不支持、恢复隐藏、开关可见；Windows 提供开关
  与恢复按钮）；**确认框 2 项：取消不写配置（app.toml 字节不变、可
  重新弹框）；确认只保存偏好（system_proxy_enabled=true，授权不落
  配置字节）**。

## 验证

```sh
devenv shell -- bash -lc 'cargo fmt --manifest-path apps/desktop/Cargo.toml && \
  cargo test --manifest-path apps/desktop/Cargo.toml --locked && \
  cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings && \
  cargo build --manifest-path apps/desktop/Cargo.toml --locked'
```

结果（2026-11-13，`nsh-validation` 工作区）：

- `cargo fmt --check`：通过。
- `cargo test --locked`：**229 项通过、1 项 ignored**（194 存量 + 35
  新增；lib 161 + headless 68）。
- `cargo clippy --locked --all-targets -- -D warnings`：通过。
- `cargo build --locked`：通过。
- 根 `cargo fmt --all --check`、`git diff --check`、
  `scripts/check-file-size.sh`：通过。

## Windows 原生验收

使用 Rust 1.98.0/MSVC 在 Windows 本地副本完成 `build` 与严格
`clippy --all-targets -D warnings`。清理了平台条件编译产生的未使用导入/fixture
辅助项，没有放宽 lint。真实场景使用全新 TOML 数据根、GitHub 官方
`sing-box v1.14.4` 和本地订阅；按主机现状使用 mixed `17999`、API `19190`，
未触碰已有 17899 核心。

- 启动前保存 Internet Settings 与 Connections 的完整值名、类型和字节/文本基线。
  外来活动代理由普通 Acquire 正确拒绝；经真实“确认接管”对话框授予一次性
  ExplicitTakeover 后，守护 arm/握手/acquire 成功，WinINet 指向本次核心。
- 正常停止先由守护恢复，再停止核心；恢复后基线逐字段一致，核心退出。
- 强制结束守护后 GUI 显示“需要恢复”；点击“恢复系统代理”由继任守护按 owner
  精确恢复，随后正常停止成功。确认提示与恢复完成提示均以真实 UI 文本验收。
- 强制结束真实 GUI 后，独立守护自动恢复基线；最终核心、GUI、守护均退出，
  差异检查为空。测试脚本曾误选守护 PID/使用旧提示文案，均以进程角色和实际
  UI 复核纠正，没有把这些脚本问题当作产品通过证据。
- 本轮证明配置校验、进程生命周期、WinINet 接管和精确恢复；未执行经核心的
  实际转发流量矩阵，未启用系统 DNS/路由/TUN，未读取生产数据。

`windows-sys` 仅按实际 API 增加 feature，未升级版本。Mihomo 真实 `-t`/启停、
WinINet 客户端实际转发与 PAC/WPAD 等仍待后续专项验收。
