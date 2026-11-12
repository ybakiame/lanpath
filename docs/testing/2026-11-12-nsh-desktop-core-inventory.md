# NSH Desktop 核心库存与下载/选择切片验证（2026-11-12）

## 范围

`apps/desktop` 第七个切片：核心库存与下载/选择，从共享
`crates/lanpath-client/src/core_management.rs`（`CoreManagementService::experimental`）
迁移语义，交互参考 `apps/gpui/src/core_management_view`（未改动 GPUI 产品代码）。
apps/nsh 不动；不启动核心、系统代理或 TUN，不读取其他应用数据。只做核心文件
管理：不做核心启动/停止/配置校验，不接核心日志流，不做环境核心覆盖/内置种子。

## 统一 TOML 布局（breaking change，不做旧数据迁移）

用户明确 GPUI 已完整迁移 TOML，且本应用是无实际用户的本地迭代产品、允许
breaking change、不需考虑旧测试数据兼容。`apps/desktop` 数据根以统一 TOML
布局为权威布局（与 GPUI 一致）：`app.toml` 承载 `[client]`/`[core]`，
`subscriptions.toml` 承载订阅清单，订阅原文/节点缓存在 `cache/subscriptions/`，
覆写模板远程缓存在 `cache/profiles/`；`profiles.json`、`config_slices.json`、
`logs/` 保持原样；外观存于 NSH storage。

统一存储初始化（`src/storage_init.rs`）在 `DataRoot` 已验证归属/树并持独占
锁后运行（`DataRoot::open` 内、建日志/缓存目录之前）：新根直接建 `app.toml`
（`[client]` 新装默认：直连 `request_proxy_mode=none`、`mitm_enabled=false`、
`data_dir` 本根；`[core]` 核心选择由共享 `settings`/`core_management` 正常
管理）与 `subscriptions.toml`；已有统一根原样保留、绝不重复改写。存在早期
切片的 JSON 测试布局（`client.json`/`subscriptions.json`/
`subscription_cache`/`profile_cache`）时明确报“旧测试配置布局不受支持，
请使用新的数据根”——不迁移、不静默忽略或清空旧文件，也不读取其他应用的
数据根。

共享 `core_management` 保持 `experimental` 原语义：写选择统一
`ensure_app_settings` + `app.toml`（无 legacy 分支，不改公共函数签名）；
共享测试为 HEAD 布局（unified TOML 与选择字段合并覆盖）。

## 业务（`src/core_inventory.rs`）

`CoreInventory` 薄封装：`CoreManagementService::experimental(root)`（绝不使用
非 confined 的 `new` 绕过库存路径/链接/版本安全），克隆廉价，所有同步清单/配置
读写经数据根 `business_guard`，网络 await 不持锁（共享库每次请求自行读取已保存
请求策略快照）。操作：`active_core`（空选中 `Ok(None)`，不当错误）、
`list_local`、`remote_channels`（过滤 stable/prerelease）、`select`、
`download`（共享语义：验证 + 自动选中 + 按通道保留当前与前一版本；进度回调只回
传共享 `CoreDownloadStage`）、`delete`（共享层保护 active）。选择持久化固定写
`app.toml`：新装默认（直连 None）由数据根初始化持久化，共享 persist_selection
读-改-写 `[core]` 段只改 `core_binary`/`core_type`，active 订阅、请求代理、
端口等全部保留；core_type 由 `[core].kind` 投影（Mihomo 只能来自 Mihomo
核心选择）。

跨页面/跨句柄 mutation 互斥：UI busy 只管本页按钮——离页后新页面的新句柄
busy 为假，若只靠 UI 层，旧后台下载在途时新下载/选择/删除会并发写同一库存。
业务层按数据根共享一把 `tokio::sync::Mutex<()>`（Weak 缓存，同根所有
`CoreInventory` 句柄竞争同一把）：`download`/`select`/`delete` 进入时
`try_acquire`，在途时立即返回“另一项核心库存操作正在进行，请稍后重试”，
绝不排队；锁覆盖整个 mutation（tokio guard 跨 await）；只读操作
（active/list/remote 查询）不加锁。`DataRoot::business_guard` 仍是短临界区
专用、绝不跨网络 await；取锁顺序恒为 mutation → business。离页只取消 UI
轮询，在途后台 mutation 继续完成并释放锁，之后任何句柄恢复可操作。下载
不包含 wintun/TUN 资源：confined 策略向共享层 `download_with_tun(requested,
!confined)` 传入不取 TUN 附件，TUN 资源将来单独处理。
release metadata API 注入：环境变量 `LANPATH_DESKTOP_CORE_API_BASE` 优先，
其次 NSH storage 键 `lanpath_desktop_nsh.core_api_base`（headless 每场景经内存
storage 预置）；未设置时共享库默认 GitHub。

## UI（`src/core_view.rs`）

侧栏新增“核心”页。固定文案“仅管理本数据根 cores/ 下的核心文件：下载、选择、
删除 · 不启动核心/系统代理/TUN”。“当前使用”卡片独立显示实际选中（与浏览类型
无关），为空显示“尚未选择核心”（配置页预览已支持 Mihomo，不再提示暂不支持）。
库存卡片：sing-box/Mihomo 类型单选（切换即刷新本地列表并清空远端
结果）、稳定版/预发布版通道单选（beta 并入预发布展示，与共享通道模型一致；
本地行按通道过滤、版本降序——排序仅展示用，校验规则全在共享层）、“检查更新”
（busy 单飞，失败保留旧状态并显示真实错误）、“下载 vX 并选中”（只针对已查询到
的版本；已安装则显示“重新校验并选中”）、本地版本行（active 行无操作按钮；
非 active 行“使用”真实探测并保存、“删除”弹确认框）。

任务模型：busy 全局单飞（begin_work 守卫，操作在途时重复按下 no-op）；worker
在 `background`（tokio 线程）执行，进度经 `Arc<Mutex>` 槽位回报共享真实阶段，
UI 任务按真实时间轮询回写 Signal（不伪造 byte 百分比）；spawn 挂在页面级
owner 下。完成/失败都复位 busy；失败保留旧列表与旧选中，只显示真实错误
（包括业务层 mutation 锁的“另一项核心库存操作正在进行，请稍后重试”）。

## NSH 0.5.0 引擎要点（本切片实测）

- `spawn` 的任务挂在 `Owner::current()` 下：在 `when(!busy)` 分支或弹窗内
  spawn，owner 随 busy 翻转/弹窗关闭被销毁，任务在首 poll 前即被取消。本页
  在构建时捕获页面级 `Owner`（`CorePageState::owner`），spawn 经
  `owner.run(..)` 执行；页面 owner 随离页销毁——在途任务取消、重进页面全新
  状态，不永久 busy。
- headless 运行手动时钟：`timer()`（engine Delay）不推进；轮询等待用
  `tokio::time::sleep`（真实时间，到点经引擎 waker 唤醒任务）。
- `background()` 需在 `spawn` 任务内调用（engine 事件线程无 tokio 上下文）。
- headless 按 test_id 按压要求该视图自身（或祖先）可按压；`button()` 是
  组件、没有 `test_id` 方法（`host::set`/`Interactive` 不外露），且 Windows
  UIA InvokePattern 只触发 button 组件自身的事件、不冒泡到外层 view——所以
  按钮的正确结构是 `on_press` 挂在 `button()` 本体、test_id 放在 button
  **内部**的包装 view 上（radio 同款：headless 命中内部 view 后 up_to 找到
  button 的 Press，UIA Invoke 命中组件事件）。纯 view 开关（enabled_switch）
  可用 view 自身 on_press + test_id。

## 验证

```sh
devenv shell -- bash -lc 'cargo fmt --manifest-path apps/desktop/Cargo.toml && \
  cargo test --manifest-path apps/desktop/Cargo.toml --locked && \
  cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings && \
  cargo build --manifest-path apps/desktop/Cargo.toml --locked'
```

- fmt：通过，无 diff。
- `cargo test --locked`：157 项通过、1 项 ignored（跨进程锁争夺子进程用例，
  由父用例以子进程驱动并通过）。其中 `src/` 96 项（数据根准入 20、请求设置
  业务 7、订阅业务 20、覆写模板业务 11、核心库存业务 10、配置合成业务 14、
  统一存储初始化 2、日志缓冲 3、采集 2、磁盘写入 2、日志服务 5），`tests/`
  headless 61 项（核心页 8、配置页 6、请求设置页 8、订阅页 18、覆写页 6、
  日志页 12、外观与导航 3）。
- 统一存储初始化覆盖（`src/storage_init_tests.rs`）：新根直接建 `app.toml`
  （直连 None、`mitm_enabled=false`、`data_dir` 本根）与 `subscriptions.toml`，
  重开不改写字节；旧 JSON 测试布局四个入口（`client.json`/
  `subscriptions.json`/`subscription_cache`/`profile_cache`）各自明确报
  “旧测试配置布局不受支持，请使用新的数据根”，旧文件/目录不删除、
  不清空，已建统一产物保持原字节。
- 业务覆盖（本线程 `std::net` 127.0.0.1 真实 HTTP 服务器 + flate2/tar 真实
  tar.gz fixture，不依赖互联网）：空根列表/无 active 不当错误；类型切换库存
  独立、active 摘要跟随；选择保留已有请求策略/active 订阅/端口等全部字段；
  新装根选择后 `request_proxy_mode=none` 固化且只写 `app.toml`；active 删除
  拒绝、inactive 版本目录删除；远端通道过滤掉 beta 只留 stable/prerelease；
  下载真实走完 release metadata → tar.gz 解压 → 脚本核心 `-v` probe → 自动
  选中 → 同通道剪枝只留当前+前一；失败（502）保留旧选择与旧 `app.toml`
  字节；Mihomo 走共享命名（`mihomo-linux-amd64-v2-v1.19.29.gz`）gzip 解压
  probe 验证选中；同根双句柄在途下载时下载/选择/删除立即报同一 busy 文案、
  只读列表不被阻塞、完成后锁释放恢复可操作。
- headless 覆盖：页面说明与空态（“尚未选择核心”不是崩溃）；active 独立显示
  与类型切换互不影响；查询→下载真实落盘（`cores/sing-box/1.2.3/sing-box`
  存在、可执行、app.toml 选中且新装默认直连、请求计数各一次）；查询失败
  保留“远程：尚未检查”并显示真实错误、换新服务后同页恢复；busy 单飞（重复
  点击只产生一次资产请求）；下载中途离页再回不永久 busy、可查询，旧下载在
  后台时新页面再下载被业务层锁立即拒绝（真实 busy 错误、零额外资产请求）、
  旧下载完成后锁释放可再下载（缓存重新校验并选中）；删除需确认（取消保留
  文件）、active 行无删除/使用按钮；使用本地版本切换 active 且配置其余字段
  不被改写；Mihomo 选中摘要（配置页预览已支持 Mihomo，不再提示暂不支持）；
  请求设置页“只读不写盘”明确为启动初始化已建 `app.toml`、页面不额外改写
  文件字节。
- clippy `--all-targets -D warnings`、`cargo build --locked`：通过；
  `apps/desktop` 主依赖新增 `tokio = { version = "1", default-features = false, features = ["sync", "time"] }`（同根 mutation 锁 + UI 任务真实时间轮询）与
  `serde_yaml = "0.9"`（Mihomo 预览序列化），dev 依赖新增
  `flate2`/`tar`（归档 fixture），`Cargo.lock` 相应更新（版本均沿用已有锁定）。
- 共享 `cargo test -p lanpath-client core_management --locked`：19 项通过、
  1 项 ignored（真实 GitHub Mihomo Alpha 下载）；`cargo clippy -p lanpath-client
  --all-targets -- -D warnings` 通过。

## 环境备注

仓库 rust-toolchain 固定 1.98.0；验证一律在 `devenv shell --` 内执行。headless
的 `wait_for` 裸字符串按整节点文本精确匹配，子串等待需用 `text:/…/` 选择器。
预置种子的场景先用应用初始化数据根再写库存/配置，否则准入拒绝无归属标记的
非空目录。

## Windows 原生验收

在 Windows 本地副本使用 Rust 1.98.0 / MSVC 构建成功，以全新独立 TOML 数据根和
UI Automation 验收真实窗口。按用户要求，本次不注入本地 Release API，直接访问真实
GitHub：

- 查询实际稳定通道得到 `sing-box v1.14.4`，下载官方 Windows ZIP、解压到本测试根
  `cores/sing-box/1.14.4/`，真实执行版本探测并自动选中，页面显示当前版本。
- 选择只写统一 `app.toml` 的 `[core]` 与 `[client]`，新装请求策略保持直连，没有生成
  `client.json`；官方二进制路径规范化后仍位于本测试库存。
- 切换到 Mihomo 库存浏览不改变实际 sing-box 选择。原生验收还发现并修复了按钮处理器
  误挂在外层 view、导致 UIA Invoke 不执行的问题；修复后查询/下载均由真实按钮完成。
- 通过 `WM_CLOSE` 正常退出，没有遗留 sing-box 进程。本切片不下载 wintun/TUN 资源，
  不启动核心、系统代理或 TUN。

自动回归仍使用本地归档 fixture 以覆盖失败、并发、切换和删除；真实 GitHub 验收不
重复执行这些破坏性用例。共享源码在本轮最终 diff 中没有改动，Windows 已同步同一实现。
