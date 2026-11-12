# NSH Desktop 订阅管理切片验证（2026-11-12）

## 范围

`apps/desktop` 第四个切片：从 apps/gpui 迁移订阅编辑、联网刷新、当前生效订阅与
原文查看（`business/subscriptions.rs`、`business/state.rs`、`subscriptions_view`），
适配 NSH 桌面独立数据根与纯 Rust NSH 组件。apps/gpui 仅作迁移来源、未修改；
apps/nsh 不动；仍不启动核心、系统代理或 TUN，不读取其他应用数据。

- 编辑：名称/规范化校验 URL/可选 User-Agent（空白归一化 `None`）；URL 变更沿用共享
  `store.update` 清除旧原文/缓存与上次结果，URL 不变保留。编辑/刷新失败显示真实错误，
  不把内存列表伪报成功。
- 刷新：新增成功后自动后台刷新（GPUI parity，已在 UI/README 明确）；手动刷新始终
  使用已保存 URL/UA（有 headless 用例证明未保存草稿不会发出）。请求走
  `fetch_subscription_for_config` + `ClientConfig::load_for_requests`，在
  `background`（tokio 线程）执行，UI 线程只做信号回写；成功持久化
  node_count/format/userinfo/error、`SubscriptionSource` 与 `CachedSubscriptionContent`，
  页面重开可读原文；失败写回共享层脱敏真实错误（不含订阅地址）且保留旧缓存；
  写回前重新比对 id/URL/UA，刷新期间被编辑/删除的结果判定 Stale 丢弃。清单/配置
  读写经业务层进程内互斥串行化（刷新的后台写回与 UI 线程的同步操作排队），
  业务句柄按规范根路径 Weak 缓存、同根共享同一把写锁；刷新全局单飞（在途时新的
  刷新不启动，完成后再可刷新）。删除与 URL 修改使缓存失效。
- 当前生效订阅：`enabled` 订阅可设为 active，持久化到本应用 `client.json` 的
  `active_subscription_id`（`load_for_requests` 把 `data_dir` 固定为本应用根，
  不信任文件里的旧路径）；拒绝不存在或停用项；停用/删除生效项自动清除；
  与 `enabled` 语义分离。已有配置原样保留 `request_proxy_mode`/
  `request_proxy_url`/`github_proxy_prefix` 等请求字段（active 的保存不改写它们，
  刷新遵守已保存策略，有 Custom 代理实测用例）；只有新装（无 `client.json`/
  `app.toml`）才设直连 `None`，与 GPUI 为新配置设定的策略一致，避免环境系统代理
  转发订阅地址。
- UI：`subscriptions_view` 拆为 add_form/rows/editor/source_view 子模块。编辑弹窗
  未保存取消需确认；刷新 busy 与结果反馈；active 标记与“设为生效”入口；原文只读
  弹窗（无原文明确“请先刷新”）；保留新增/启停/删除确认。编辑草稿与列表数据分离，
  刷新/启停/删除/设置生效不清空草稿（有在途刷新场景用例）。异步完成的消息按交互
  序号抑制，晚到的刷新结果不覆盖后续操作的反馈；行内错误随记录持久化展示。
- 日志：只记录操作结果与共享层脱敏错误（刷新失败/成功节点数、编辑/删除/生效/启停），
  不含订阅 URL 与正文。

## 验证

```sh
devenv shell -- bash -lc 'cargo fmt --manifest-path apps/desktop/Cargo.toml && \
  cargo test --manifest-path apps/desktop/Cargo.toml --locked && \
  cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings && \
  cargo build --manifest-path apps/desktop/Cargo.toml --locked'
```

- fmt：通过，无 diff。
- `cargo test --locked`：83 项通过、1 项 ignored（跨进程锁争夺子进程用例，由父用例
  以子进程驱动并通过）。其中 `src/` 50 项（数据根准入 20、订阅业务 18 项含本地
  HTTP server 刷新/active/配置保留用例、日志缓冲 3、采集 2、磁盘写入 2、日志服务 5），
  `tests/` headless 33 项（订阅页 18、日志页 12、外观与导航 3）。
- 业务覆盖（`std::net` 本线程 127.0.0.1 HTTP server，不依赖互联网）：编辑保留/
  清除缓存、UA 归一化、非法输入拒绝；刷新发送已保存 UA（默认 `clash.meta`）、
  userinfo 解析持久化、原文与缓存写盘并重开读取、失败（连接拒绝/HTTP 500）保旧
  缓存、在途编辑后结果判 Stale 不覆盖；active 设置校验/持久化/停用与删除清除、
  `data_dir` 固定与新装代理模式断言；同根重开 `Arc::ptr_eq` 共享句柄、已有自定义
  请求配置经 set_active/set_enabled/remove/active_id 后字段原样保留、刷新按已保存
  Custom 代理策略发出（绝对形式请求到达代理服务器）。
- headless 覆盖（`nsh_testing::Headless`，每场景独立临时数据根 + 桌面视口）：
  新增自动刷新 busy/结果反馈与节点数（本地服务器，断言实际请求路径与 UA）、重复
  刷新去重（延迟路由下双按只产生一次请求）、手动刷新只用已保存 URL/UA、编辑保存
  持久化且不触发联网、改 URL 后缓存失效与“请先刷新”、取消保护双确认、在途刷新
  时草稿留存、原文弹窗只读真实内容、失败真实错误（HTTP 500）不伪报成功、active
  设置/持久化/停用拒绝/删除清除跨重开；全局单飞（两订阅同时刷新只有一个请求、
  busy 归零、完成后第二个可刷新）。
- clippy `--all-targets -D warnings`、`cargo build --locked`：通过。

## 环境备注

仓库 rust-toolchain 固定 1.98.0；验证一律在 `devenv shell --` 内执行
（Nix rustup + 一致 glibc）。新增 dev-dependency `tokio = "1"`
（`default-features = false, features = ["macros", "rt", "time"]`，仅测试用），
apps/desktop/Cargo.lock 相应刷新（tokio 版本与锁内既有 1.52.3 一致）。

## Windows 原生验收

在 Windows 本地副本使用 Rust 1.98.0 / MSVC 执行 `cargo build --locked` 成功。以独立
临时数据根启动真实应用，通过 UI Automation 操作原生窗口；网络仅访问测试进程持有的
`HttpListener`（127.0.0.1 随机空闲端口），不依赖互联网，不读取 Tauri/GPUI 数据：

- 新增后自动请求 `/first.yaml`，实际 UA 为 `NativeAgent/1.0`；显示 1 个真实解析节点、
  设置当前生效、打开只读原文弹窗并核对完整响应正文。
- 编辑名称、URL 与 UA 后保存不联网，旧原文提示需先刷新；手动请求 `/second.yaml`，
  实际 UA 为 `NativeAgent/2.0`，active 保留。重命名后开关使用稳定 UUID，启停正常。
- 未保存取消先确认，“继续编辑”保留草稿，“放弃修改”返回已保存名称。
- 对已有成功缓存的订阅返回真实 HTTP 500：错误上屏，节点元数据与上次原文仍可读。
- 停用当前订阅清除 `client.json` 的 active；确认删除后列表为空、订阅缓存文件清空；
  `WM_CLOSE` 正常退出。

headless 的异步/存储断言与上述 Windows 原生交互互补；本切片没有系统保存对话框。
