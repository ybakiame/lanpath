# NSH Desktop 加密备份与原子恢复切片验证（2026-11-13）

## 范围

`apps/desktop` 第十个切片：纯 Rust NSH 桌面的“加密备份与原子恢复”。
语义全部复用共享 `crates/lanpath-client/src/backup/`（`mod/service/
preview/transaction/snapshot/archive`），本切片不复制、不修改共享加密与
事务实现；外观按本页格式固定 `None`（NSH 外观 storage 不属于备份格式）。

- 业务壳 `apps/desktop/src/backup.rs`：`Backup`（按数据根 Weak 缓存）
  包装共享 `BackupService`——导出/写出/限长读入/解密预览/一次性 take/
  discard/restored/acknowledge/is_pending，以及确认后的
  `prepare_confirmed_restore`（消费授权 → 活动屏障写保护 → 停止核心 →
  业务锁内共享 `RestoreInput::prepare`）。
- 页状态与 UI `apps/desktop/src/backup_view.rs` +
  `backup_view/{export_card,import_card}.rs`：掩码口令、原生保存/打开
  对话框、一次性预览展示与 300 秒过期调度、恢复确认弹窗、待重启横幅
  与 NSH 可靠重启入口（`nsh_core::element::desktop::restart`）。
- 数据根 `apps/desktop/src/data_root.rs`：新增异步活动屏障
  （`tokio::sync::RwLock`，跨 await 长任务持读保护）与恢复 pending 门
  （`ensure_writable`，业务锁内查 `.backup-pending`）；`DataRoot::open`
  在归属/树/独占锁之后、storage_init 与任何业务之前先
  `apply_pending_at_startup`（共享启动事务，提交/回滚由其保证；恢复
  日志含糊即返回真实错误）。
- 写静默接线：订阅（add/update/set_enabled/remove/set_active/刷新入口
  与写回）、覆写模板（save/remove）、请求设置（save）、系统代理偏好
  （save_preference）、核心库存（select/delete/download）、核心启动
  （start_impl）全部过 pending 门；订阅刷新/配置合成/核心下载/核心启动
  持活动读保护跨 await；配置合成内订阅刷新改用无保护
  `refresh_with_config_inner`（已持读保护时重复获取读锁在写者排队时
  会死锁）。取锁顺序统一为 activity → mutation → lifecycle →
  proxy_guardian → business，无反向路径；守护 RPC 不持任何上述锁。
- App 启动门（`src/lib.rs`）：日志服务打开失败（数据根被占用/准入
  拒绝/恢复事务冲突）不再静默降级——全窗真实错误，不进入任何业务页，
  半恢复状态绝不暴露。

**跨进程重启与 Windows 原生 Open/SaveDialog 验收待主代理执行**：
本切片自动化测试全部在 Linux headless/本线程服务器上运行，原生对话框
经 headless 注入应答；NSH 可靠重启 API（旧进程退出后才拉起新进程）与
启动事务在真机 Windows 的完整链路未在本机验收，不据此宣称已验证。

## 启动事务顺序（明确）

1. `DataRoot::open`：归属/树/独占文件锁全部就绪；
2. **同锁内** `apply_pending_at_startup`（无 pending 是两条例行探测）：
   提交 → 原子换装 `.backup-applied.json`；校验失败 → pending 改名
   `.backup-rejected-*` + 写 `.backup-failed`（原配置保留，启动继续）；
   恢复日志含糊 → 真实 Err（App 启动门全窗失败）；
3. `storage_init`（统一 TOML；旧 JSON 布局拒绝）→ logs/cache/身份；
4. 日志/核心运行时/页面按既有顺序打开；备份页读取
   `.backup-applied.json`/`.backup-failed` 上屏。

恢复准备顺序：消费一次性 preview 授权（失败即返回、无状态变更）→
活动屏障写保护（等在途读保护清零）→ 再查 pending → 停止核心（守护先
按原值恢复）→ 业务锁内共享 prepare（回滚加密写入 `backup-history/`、
暂存 rename 为 `.backup-pending`）→ 释放。任一步失败不遗留 pending，
可重新解密重试；成功后写门生效直到重启应用。

## 测试

新增 17 项，全部真实文件/真实 age 加密，不 mock 成功：

- 业务 9 项（`src/backup_tests.rs`）：
  - 导出真实 age 往返（`age-encryption.org` 头、共享 decode 摘要与
    清单一致）、错误密码拒绝、弱口令（<12）拒绝；
  - 写出绝不覆盖（二次写出错误、原字节不变）、失败清理半成品；
  - 导入 16 MiB 上限（稀疏超限文件按元数据拒绝）；
  - inspect 错误密码脱敏（不含口令子串）、preview 一次性（take 后
    二次消费拒绝）、discard 后立即失效、二次 inspect 生成新 id；
  - 准备恢复等待在途刷新：500ms 延迟服务器 + `tokio::join!` 并发，
    断言总耗时 ≥ 延迟（写保护确实等在途读保护）、刷新真实写回
    （2 节点）、回滚捕获发生在写回之后（`subscriptions.toml` 含
    `node_count = 2`）；
  - 运行中核心被停止（fixture 核心真实启动 → prepare →
    `is_running_sync()==false` 且 mixed 端口释放）；
  - pending 写门（新增订阅/覆写/请求设置/刷新全部拒绝、读允许）、
    重复 prepare 拒绝、共享 `apply_pending_at_startup` 原子应用后
    订阅恢复、restored 摘要 + acknowledge 清除、写恢复；
  - 失败 prepare（无效 preview_id）不遗留 pending，重试成功；
  - 损坏 pending（版本 99）：启动事务真实失败、`.backup-failed`
    落盘、原配置完好、restored() 返回真实错误、acknowledge 可清除。
- headless 8 项（`tests/backup_page.rs`，`nsh_testing::Headless` +
  内存 storage 独立数据根）：
  - 导航展示导出/导入分区与作用域说明；
  - 导出弱口令错误上屏；强口令经注入保存对话框写出真实 `.zip.age`
    （共享 decode 验证版本 v3），成功后口令输入清空；
  - “生成强密码”一键填入 24 位字母数字；
  - 导出目标已存在：真实错误、哨兵字节不变；
  - 导入经注入打开对话框选文件、错误密码脱敏失败（屏幕无口令泄漏）、
    正确密码摘要上屏、修改密码预览立即消失；
  - 端到端：新增订阅（本地服务器真实刷新 2 节点）→ 导出 → 删除订阅 →
    导入预览（订阅 1）→ 确认恢复 → 待重启横幅、导出/导入卡片隐藏、
    磁盘 `.backup-pending` 与 `backup-history/Lanpath-rollback-*` 存在、
    订阅页新增被拒（“已准备恢复备份”上屏）、重启入口存在（不按下）；
  - 启动恢复摘要上屏（盘上经共享事务预置 `.backup-applied.json`），
    acknowledge 后消失且标记文件删除；
  - `.backup-failed` 真实错误横幅（原配置保留、重新导入指引）。

共享 backup 用例（一次性/过期/事务回滚等）在
`crates/lanpath-client/src/backup/{tests,service_tests}.rs` 既有覆盖，
本切片复用未改共享代码。

边界说明：单元/headless 测试无法跨进程重启——恢复“重启后应用”在测试
内经共享 `apply_pending_at_startup` 直接承担（与 `DataRoot::open` 内
调用同一函数）；进程内 ROOTS 缓存使同路径不重复准入，不影响上述等价性。
密码框掩码（`.masked(true)`）为平台原生属性，headless 不做渲染断言。

## 验证

```sh
devenv shell -- bash -lc 'cargo fmt --manifest-path apps/desktop/Cargo.toml && \
  cargo test --manifest-path apps/desktop/Cargo.toml --locked && \
  cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings && \
  cargo build --manifest-path apps/desktop/Cargo.toml --locked'
```

结果（2026-11-13，`nsh-validation` 工作区）：

- `cargo fmt --check`：通过。
- `cargo clippy --locked --all-targets -- -D warnings`：通过。
- `cargo build --locked`：通过。
- `cargo test --locked`：**246 项通过、0 失败、1 项 ignored**
  （229 存量 + 17 新增；lib 170 = 161 + 9，headless 76 = 68 + 8）。
  age scrypt（work factor 17）加密较重，备份用例单线程约 20 秒级。
- 根 `cargo fmt --all --check`、`git diff --check`、
  `scripts/check-file-size.sh`（新增文件逐个校验）：通过
  （`import_card.rs` 465 行、`data_root.rs` 480 行，均为预警线内、上限内）。

## Windows 原生验收

在 Windows 本地副本完成 Rust 1.98.0/MSVC 构建与严格 Clippy。使用全新 TOML
数据根、一个真实本地订阅和一个覆写模板，验证系统原生文件对话框与真实重启：

- 通过 SaveDialog 导出 `.zip.age`，再由 OpenDialog 选中同一文件。文件名编辑
  使用 `1148` 宿主内部的原生编辑框与 `WM_SETTEXT`，不经过输入法；打开后页面
  显示实际选中的文件名。
- 错误密码显示脱敏失败，正确密码在页面下方内联显示订阅/覆写/文件摘要和
  “恢复此备份…”按钮（不是独立预览弹窗）。
- 确认恢复后生成 `.backup-pending` 和加密回滚；待重启时新增订阅被真实拒绝。
  点击“立即重启应用”后旧进程退出、新进程启动，启动事务应用备份，显示恢复摘要；
  “我知道了”清除标记，回滚文件保留。
- 扫描磁盘日志，测试密码未出现。测试辅助脚本的路径/IME/可见文本定位问题均
  按实际 UI 纠正，未将测试 ID 查找失败误报为产品预览缺失。

本轮未再次启动核心，故没有新增核心/系统代理状态变化；恢复前 `stop` 的实际
守护归还原路径已由上一切片覆盖。真实中断/崩溃下的启动恢复沿用共享事务测试，
未做断电故障注入。
