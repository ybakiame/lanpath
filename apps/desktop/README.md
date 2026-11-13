# Lanpath NSH Desktop

`apps/desktop` 是 NSH Windows 迁移目标（纯 Rust，NSH 0.5.0），与最小验证应用 `apps/nsh`、
GPUI 对照 `apps/gpui` 并行存在。应用身份 `com.ybakiame.lanpath.desktop.nsh`，不读取或迁移
Tauri/GPUI/NSH 验证应用的数据根；核心“校验并启动”仅限用户态 loopback mixed + Clash API；
Windows 系统代理经独立普通权限同映像守护精确接管/恢复（见核心页“系统代理”分区），GUI 侧
绝不直写注册表；TUN/VPN 不接管，不安装服务/启动项；仅订阅新增/刷新与核心查询/下载按需联网。

窗口壳为侧栏导航 + 主内容区。当前实现七个真实页面：

- 请求设置：订阅刷新与远程资源请求的网络请求策略（从 apps/gpui 迁移并适配）。
  四种代理模式（直连 / 跟随系统 / 核心代理 / 自定义 HTTP(S)/SOCKS5）单选持久化到本应用
  `app.toml` 的 `[client]` 段（`request_proxy_mode` / `request_proxy_url` /
  `github_proxy_prefix`）；自定义地址与 GitHub 镜像前缀草稿可编辑，保存经共享
  `validate_request_proxy` 校验，非法地址（scheme 非 http/https/socks5/socks5h、带
  path/query/fragment、端口 0 等）拒绝不落盘，错误不回显用户名/密码/token/完整 URL；
  非自定义模式同样保存自定义地址草稿供稍后切回（与 GPUI 一致）。保存时在同一业务锁内
  重新加载最新配置，只改请求相关三个字段，不覆盖 active 选择、system/tun、MITM、端口等
  无关字段；读取页面不无故写盘（数据根初始化已建 `app.toml`，页面自身不额外改写文件
  字节），新装默认直连（数据根初始化持久化，`load_for_requests` 固定 data_dir 本根），
  已有配置损坏返回真实错误、绝不重写为默认。GitHub 前缀按共享
  `apply_github_proxy_prefix` 语义在使用时拼接（保存值原样保留）。保存后
  `ClientConfig::load_for_requests` 立即可读，订阅刷新经共享 `load_request_config`
  消费同一策略（无第二处新装默认）。核心代理只指向已保存的 mixed_port：
  核心未运行时请求真实失败（页面如实说明，不做连接测试、不显示假成功；
  系统代理接管开关在核心页“系统代理”分区，不在本页）。重置恢复已保存草稿
  （有未保存修改需确认），日志只记模式不记地址/前缀。
- 订阅：真实订阅列表与持久化。复用共享 `lanpath-client::SubscriptionStore` /
  `Subscription` / `SubFormat`，数据写入本应用自己的隔离数据根（所有权标记 +
  进程内文件锁租约 + 生产/链接路径准入，从 apps/gpui 迁移并适配，约束未削弱）。
  支持名称 + URL + 可选 User-Agent 新增（校验失败不覆盖已有数据，新增成功后自动
  后台刷新，沿用 GPUI 行为；新增订阅无覆写绑定）、编辑（URL 变更清除旧原文/缓存，
  URL 不变保留；弹窗内可绑定“无覆写”或一个现存覆写模板——单选列表真实可选，保存时
  业务层在同一业务锁内校验模板存在，失效引用显示真实错误、不静默改绑；未保存取消
  需确认）、手动刷新（busy/结果反馈；始终使用已保存 URL/UA，不把未保存表单写回；
  失败显示共享层脱敏真实错误并保留旧缓存；刷新期间被编辑/删除的结果丢弃不覆盖；
  刷新/启停/设置生效不改变覆写绑定）、启用切换、设为当前生效订阅（持久化到本应用
  `app.toml` 的 `active_subscription_id`，拒绝停用/不存在项，停用或删除生效项自动
  清除）、查看最后一次成功刷新原文（只读弹窗，无原文时明确提示先刷新）与带确认的
  删除；行内显示绑定的覆写模板名，模板被外部删除等导致引用失效时显示真实错误提示。
  联网仅在新增/刷新时按需发生：已有配置原样保留并遵守已保存的请求代理策略
  （`request_proxy_mode`/`request_proxy_url`/`github_proxy_prefix`），新装默认直连、
  不读取系统代理（数据根初始化已把该默认持久化到 `app.toml`，与 GPUI 为新配置设定
  的策略一致）；请求在后台线程执行，不启动核心、系统代理或 TUN。清单/配置的读写与覆写
  模板业务共用数据根的同一把业务锁（同根句柄经 Weak 缓存共享数据根），刷新全局单飞，
  完成后再可刷新。
- 覆写：真实覆写模板（Profile）管理。复用共享 `lanpath-client::ProfileStoreV2`，
  模板存本数据根 `profiles.json`（不读取/迁移其他应用数据，含旧版
  `profile.json`——本应用独立新根不会出现该文件）。支持列出、新建、编辑、删除
  （需确认；被任一订阅 `profile_id` 引用时业务层在同一业务锁内拒绝并显示真实错误，
  解除绑定后才可删）。名称 trim 必填且除自身外唯一；YAML/JS 本地覆写真实调用共享
  `validate_yaml_override`/`validate_js_override`（空允许），用 NSH 真实多行输入
  编辑（保留换行）；两个远程覆写地址可选、规范化并共享 `validate_remote_url` 校验。
  保存失败保留草稿并显示真实错误，取消有未保存确认。远程覆写地址本页只保存供后续
  配置合成时解析：不下载、不预览、不显示“已生效”。日志只记名称/结果/节点数，不记
  YAML/JS 正文、远程地址或订阅地址。
- 核心：真实核心库存与下载/选择。复用共享 `lanpath-client::core_management::
  CoreManagementService::experimental`（confined 策略，与 GPUI 同一共享语义），
  只管理本数据根 `cores/<binary>/<version>/` 下的真实核心文件：列出本地真实
  版本、查询远端通道（仅 stable/prerelease）、下载指定已查询版本（共享语义：
  下载 + version probe 验证 + 自动选中 + 按通道保留当前与前一版本）、选择本地
  版本、带确认地删除非 active 版本（共享层拒绝删除 active/越界/链接核心）。
  sing-box/Mihomo 库存类型切换（实际选中独立显示，active 为空显示“尚未选择
  核心”，不把无核心当崩溃）；Mihomo 文件名/版本规范全走共享函数，不自造规则。
  选择持久化固定写 `app.toml`（统一 TOML 布局，与 GPUI 同一共享语义：共享
  persist_selection 读-改-写 `[core]` 段，只改 `core_binary`/`core_type` 两个
  字段，active 订阅、请求代理、端口等全部保留；新装默认请求策略——直连 None——
  已由数据根初始化持久化，不会被核心选择改回 System）。core_type 由
  `[core].kind` 投影：Mihomo 类型只能来自 Mihomo 核心选择。页面固定文案
  “管理本数据根 cores/ 下的核心文件 · 校验并启动为真实二进制校验后的用户态
  loopback 运行；系统代理经独立守护精确接管/恢复（见下方分区），TUN/VPN 不接管”。
  busy 全局单飞（操作按钮 busy 中按下 no-op，完成后恢复）；操作失败保留旧
  列表并显示真实错误；下载进度只显示共享真实阶段（查询资产/下载/解压/验证/
  保存），不伪造 byte 百分比；任务模型：操作在 `background` 执行，Signal 只在
  UI 线程回写（进度槽位 + 轮询），spawn 挂在页面级 owner 下（弹窗/行级临时
  owner 销毁不取消在途工作；离开页面 owner 取消任务，重进页面全新状态，
  不永久 busy）。mutation 跨页面互斥在业务层：同根所有 `CoreInventory` 句柄共享
  一把 try_acquire 锁，下载/选择/删除在途时立即报“另一项核心库存操作正在进行，
  请稍后重试”（绝不排队），锁覆盖整个 mutation（可跨 await），只读查询不加锁；
  下载不包含 wintun/TUN 资源（confined 策略不取 TUN 附件，TUN 资源将来单独
  处理）。远端查询/下载按共享库存现有实现消费已保存的 ClientConfig
  请求策略（`request_client_builder` + GitHub 镜像前缀），GitHub API base 仅
  测试可注入（storage 键 `lanpath_desktop_nsh.core_api_base` / 环境变量
  `LANPATH_DESKTOP_CORE_API_BASE`），生产仍 GitHub。日志只记类型/版本/结果
  （不记完整路径），不记 token 或完整带凭据 URL。不接核心日志流。
  核心运行（同页“运行”卡片）：新增“校验并启动”“停止”。启动固定同一份
  ClientConfig 快照——拒绝 `tun_enabled`（TUN 仍不授权，设置保留）；
  必须有 confined 库存选中核心并经
  `validate_managed_binary`；先预检 mixed/Clash API 端口 loopback 可绑定
  （新根默认端口为 GPUI 实验值 17899/19090，避免与生产 17890/9090 冲突，
  已有根保留已保存值）；配置经 `compose_with_config(快照)` 一次合成（不重复
  联网），由 `ConfigValidator` 先守能力边界（sing-box 仅允许已保存端口的
  127.0.0.1 mixed inbound，拒绝 tun/`set_system_proxy`/非 loopback/面板下载
  字段，log/cache/rule_set/certificate 资源路径限根内；Mihomo mixed-port
  匹配、不得 allow-lan/tun、external-controller 仅 loopback、无面板下载
  字段），再经真实核心二进制校验（sing-box `check -c 临时文件 -D 根` /
  Mihomo `-t -d 根 -f 临时文件`，no_window，并行有界 drain，20 秒超时强杀
  回收）；通过后才启动并等待 mixed 端口真实监听、Clash API 就绪，任何失败
  回滚 `stop_process`（回滚失败同时报告原错误与回收错误并保留可重试状态）。
  停止回执展示真实 kind。状态轮询每次真实核验进程活性，能发现核心异常退出；
  运行状态按根共享（Weak 缓存 + App 级 Stored），跨页一致；GUI 正常退出
  drop 按记录的 OS 进程身份回收本次子进程，绝不按进程名强杀。启停与库存
  mutation 共享同根 try_acquire 锁：启动途中下载/选择/删除立即 busy，运行中
  （含停止进行中 running 未清）业务层真实核验（`is_running_sync`，不看 UI
  状态）拒绝修改核心文件或选择。日志只记操作/端口/版本/阶段，不记配置正文、
  节点/代理凭据或完整 URL。核心以 sing-box 为主，Mihomo 只需能运行订阅配置。
  系统代理（同页“系统代理”分区，仅 Windows）：开关只保存偏好（同一业务锁内
  读最新配置只改 `system_proxy_enabled` 一个字段）；首次启用弹一次性确认框
  （“若已有其他代理，仅本次接管，停止/退出精确归还；授权不保存”），确认才
  保存偏好并授予进程内一次性接管授权（AtomicBool，不持久化、重启即失效；
  停用即清除；启动 acquire 时读取并消费，成功或失败都只影响当次），取消
  不写配置。运行中切换走安全启停周期（停止含守护恢复，启动按新偏好接管），
  绝不绕守护直写注册表。接管语义迁移自 apps/gpui `proxy_guardian`：同映像
  独立普通权限守护进程（管道身份 `LanpathNSH-Proxy-<uuid>`，参数有界版本化，
  SID/session/logon/elevation/映像/进程身份全核验，不继承 Job、不安装服务），
  核心校验/mixed/API 就绪后 arm 并 acquire `127.0.0.1:mixed_port`（普通
  Acquire 恒共享 RequireDirect，外来活动代理拒绝；仅用户确认后以独立
  `AcquireTakeover` 形态经共享 `acquire_with_policy(..., ExplicitTakeover)`
  接管并精确归还），接管失败保留核心与恢复证据；启动前恢复本应用
  abandoned journal；停止先守护 restore 再停核心，守护死亡/通信失败只按
  本应用 owner（数据根 UUID）精确恢复；界面显示未接管/本应用持有/需要
  恢复/未知错误，并提供仅本应用 owner 的显式“恢复系统代理”。复用共享
  `lanpath-client::sysproxy` 会话与 DPAPI 日志，不复制不修改。非 Windows
  显示“仅 Windows 支持”并隐藏恢复操作（偏好开关仍可见可保存），已保存
  偏好不产生运行失败。真实 WinINet 验收待主代理（自动化测试全部为
  fixture）。
- 配置：共享合成预览（从 apps/gpui 的 `business/config.rs` 迁移 sing-box 与
  Mihomo 两条合成分支）。用户点击“生成配置预览”后在后台线程按已保存配置快照合成
  完整配置——sing-box 输出 pretty JSON，Mihomo 输出 YAML（核心直接消费的格式），
  页面以只读滚动区域展示正文、生成警告（配置切片回退、远程覆写降级、缺失规则集、
  Mihomo 切片未应用等）与真实错误，明确文案“共享合成预览，未进行核心二进制校验”，
  绝不显示“核心已验证/已启动”。本切片不做核心下载/安装/校验/启停，不启动核心、
  系统代理或 TUN。核心以 sing-box 为主；Mihomo 现阶段支持订阅配置合成运行：
  优先原始 Clash 订阅原文提取代理（其余格式按共享转换逐节点处理，无法表示的协议
  真实报错），远程+本地覆写链与 sing-box 同一实现；不做 Mihomo 结构化切片编辑，
  用户定制的 sing-box 配置切片只警告、不应用（切片内容保留，供切回 sing-box）。
  合成前置：当前生效订阅必须存在且启用（共享存储语义）；优先本地节点缓存，无缓存时
  Core 请求策略明确拒绝，其余模式用进入网络前固定的 `ClientConfig` 快照联网刷新
  一次——网络途中设置页的修改不改变本次合成策略（`Subscriptions::refresh_with_config`
  与 `refresh` 共享同一实现，写回按 id/URL/UA 防 stale，业务锁不跨 await）。关联
  Profile 必须真实存在，远程 YAML/JS 经共享 `resolve_remote_overrides_with_context`
  与本根 `cache/profiles` 解析，远程失败按共享降级并展示真实警告；配置切片读取失败
  按 GPUI 回退共享默认值并把真实警告展示给用户。合成链与 GPUI 启动前预览同路径：
  sing-box 走 `build_core_config_v2` + `compose_singbox_config(mixed_port, None)`，
  规则集走 `prepare_missing_rule_sets_warn_only`/`prepare_builtin_rule_sets`/
  `materialize_rule_sets`，缺失按共享降级并回报 missing tags；`inject_local_override_warn_only`、
  `apply_panel_features` 后移除 `experimental.clash_api` 的 `external_ui`/
  `external_ui_download_url`；Mihomo 走 `mihomo_nodes_from_subscription` +
  `build_mihomo_config_v2` + `compose_mihomo_config(mixed_port, None)` +
  `apply_mihomo_panel_features`，并移除顶层 `external-ui`/`external-ui-url`，
  避免预览触发面板下载。任务模型：busy 单飞（在途重复点击不启动第二个合成，
  完成后可再生成）；失败保留旧预览并显示真实错误，不伪报新成功；离开页面在途任务
  随 owner 取消，重进页面不永久 busy。日志只记节点数、警告数与结果，不记配置正文、
  节点凭据、远程 URL 或代理 URL。
- 日志：本应用真实运行事件。内存分区是有界缓冲（1000 条、revision 增量快照、
  级别筛选、显示条数选择、2 秒轮询可暂停、行详情、带确认的清空）；采集经 tracing
  全局层只捕获 `lanpath_*` target 的本应用事件。磁盘分区读取本应用数据根 `logs/`
  的真实文件列表与尾部原文（共享 `lanpath-client::logs` 安全读取：白名单文件名、
  拒绝链接/硬链接、2 MiB / 1000 行上限），导出经 NSH 原生保存对话框选择目标后拼接
  原文写出。磁盘写入为 `app.log.YYYY-MM-DD` 日轮转（tracing-appender 非阻塞 +
  队列上限 + 真实错误上报，退出冲刷）。无核心日志流：本应用不启动核心。
- 外观：浅色 / 深色 / 跟随系统切换，真实调用 `set_color_scheme`，选择持久化到 NSH storage。

订阅页底部保留“共享解析验证”卡片：调用共享 `lanpath-client::parse_subscription_body`
解析固定测试订阅，作为独立的共享解析验证，与订阅列表数据无关。

## 数据根

- 默认根为 NSH 桌面独立身份目录（Windows：`%LOCALAPPDATA%\lanpath-desktop-nsh`；
  Linux/macOS：`$XDG_DATA_HOME/lanpath-desktop-nsh` 或 `~/.local/share/lanpath-desktop-nsh`）。
- 显式覆盖：环境变量 `LANPATH_DESKTOP_DATA_DIR` 优先；宿主/测试也可通过 NSH storage
  键 `lanpath_desktop_nsh.data_dir` 预置（优先级低于环境变量）。两者都经过同一套准入检查。
- 准入拒绝生产/便携 Lanpath 祖先、其他 Lanpath 应用数据根（Tauri/GPUI/apps/nsh）、
  符号链接/junction/硬链接共享文件；损坏的身份标记不会静默重建。不提供便携模式。
- 统一 TOML 布局是权威布局（与 GPUI 一致）：`app.toml` 承载 `[client]`（请求策略、
  `active_subscription_id` 等）与 `[core]`（核心选择）两段，`subscriptions.toml` 承载
  订阅清单，订阅原文/节点缓存在 `cache/subscriptions/`，覆写模板远程缓存在
  `cache/profiles/`；`profiles.json`、`config_slices.json`、`logs/` 等保持原样。
  **breaking change**：本应用是无实际用户的本地迭代产品，不提供旧数据迁移——
  数据根准入（归属/树/独占锁）完成后只做很小的统一存储初始化：新根直接建
  `app.toml`（`[client]` 新装默认：直连 `request_proxy_mode=none`、
  `mitm_enabled=false`、`data_dir` 本根；`[core]` 核心选择由共享
  `settings`/`core_management` 正常管理）与 `subscriptions.toml`，已有统一根
  原样保留。存在早期切片的 JSON 测试布局（`client.json` /
  `subscriptions.json` / `subscription_cache/` / `profile_cache/`）时明确报
  “旧测试配置布局不受支持，请使用新的数据根”，不迁移、不静默忽略或清空，
  也不读取其他应用的数据根。
- 订阅、请求设置、覆写模板、核心库存、配置合成与日志共用同一条数据根租约与按路径缓存：页面来回切换不重开缓冲，
  第二进程仍被拒绝。订阅清单（`subscriptions.toml`）、覆写模板（`profiles.json`）、核心库存
  （`cores/<binary>/<version>/`）与请求设置
  （`app.toml`）的所有读写共用数据根上的同一把业务锁：校验引用与落盘在同一临界区内完成，
  不会出现“校验无引用后另一线程完成绑定”的竞争；锁不跨网络 await。配置合成预览经同根
  `Subscriptions` 句柄复用这把锁与 Weak 缓存，快照刷新与页面操作天然互斥。
- 订阅原文与节点缓存在数据根 `cache/subscriptions/`（统一布局；共享存储层按
  `app.toml` 存在与否自动分派）；当前生效订阅与请求设置持久化在本应用自己的
  `app.toml`（`active_subscription_id` 与 `request_proxy_mode` / `request_proxy_url` /
  `github_proxy_prefix`），不触碰其他应用的配置。

```sh
cargo test --locked      # 全部测试：单元/数据根准入（src）+ 纯 Rust headless（tests/）
cargo run                # 本机原生后端直接运行 Rust 根组件
```

测试不依赖 QuickJS/TSX：headless 场景在 `tests/` 下用 `nsh_testing::Headless`
（内存 storage 后端预置每场景独立临时数据根，1100×800 桌面视口）直接驱动 Rust 根组件；
日志场景经真实 tracing 采集路径验证内存/磁盘/导出（保存对话框由 headless 注入应答），
订阅刷新场景经 `tests/common` 的本线程最小 HTTP 服务器（127.0.0.1 随机端口）真实联网，
断言实际请求路径与 User-Agent，不依赖互联网。核心页场景复用同一服务器：release metadata
JSON 回指本服务器资产（`{addr}` 占位符），下载经真实 tar.gz 归档解压与脚本核心 version
probe（Unix）验证解压路径/自动选中/剪枝语义；查询失败、busy 单飞与离页恢复、删除确认、
配置合并不被核心选择改写等均有覆盖；release metadata API 经 storage 键注入，生产仍
GitHub。请求设置场景复用同一服务器做正向代理：
页面保存 Custom 代理后触发订阅刷新，代理以绝对形式收到不可达目标
（`192.0.0.0/24` 文档地址）的完整请求，证明请求确实走新代理；Core 未启动负例把已保存
mixed_port 指向本机闲置端口，断言刷新真实连接拒绝、错误落盘，不等待超时。覆写模板场景
不联网：远程覆写地址只保存不下载；种子数据场景先启动一次应用初始化数据根（写入归属
标记）再预置业务文件，与生产准入一致。

`nsh.json` 命名根组件，`src/lib.rs` 是 UI，`theme.json` 沿用 `apps/nsh` 的浅色/深色
令牌。NSH 生成的文件在 `.nsh/`，Cargo 构建目录为 `target/`。
