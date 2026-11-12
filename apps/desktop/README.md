# Lanpath NSH Desktop

`apps/desktop` 是 NSH Windows 迁移目标（纯 Rust，NSH 0.5.0），与最小验证应用 `apps/nsh`、
GPUI 对照 `apps/gpui` 并行存在。应用身份 `com.ybakiame.lanpath.desktop.nsh`，不读取或迁移
Tauri/GPUI/NSH 验证应用的数据根；不启动核心、系统代理或 VPN，仅订阅新增/刷新按需联网。

窗口壳为侧栏导航 + 主内容区。当前实现五个真实页面：

- 请求设置：订阅刷新与远程资源请求的网络请求策略（从 apps/gpui 迁移并适配）。
  四种代理模式（直连 / 跟随系统 / 核心代理 / 自定义 HTTP(S)/SOCKS5）单选持久化到本应用
  `client.json`（`request_proxy_mode` / `request_proxy_url` / `github_proxy_prefix`）；
  自定义地址与 GitHub 镜像前缀草稿可编辑，保存经共享 `validate_request_proxy` 校验，
  非法地址（scheme 非 http/https/socks5/socks5h、带 path/query/fragment、端口 0 等）拒绝
  不落盘，错误不回显用户名/密码/token/完整 URL；非自定义模式同样保存自定义地址草稿供
  稍后切回（与 GPUI 一致）。保存时在同一业务锁内重新加载最新配置，只改请求相关三个
  字段，不覆盖 active 选择、system/tun、MITM、端口等无关字段；读取页面不无故写盘，
  新装默认直连（`load_for_requests` 固定 data_dir 本根），已有配置损坏返回真实错误、
  绝不重写为默认。GitHub 前缀按共享 `apply_github_proxy_prefix` 语义在使用时拼接
  （保存值原样保留）。保存后 `ClientConfig::load_for_requests` 立即可读，订阅刷新
  经共享 `load_request_config` 消费同一策略（无第二处新装默认）。核心代理只指向已保存的
  mixed_port：本切片不启动核心，核心未运行时请求真实失败（页面如实说明，不做连接测试、
  不提供系统代理/TUN 开关、不显示假成功）。重置恢复已保存草稿（有未保存修改需确认），
  日志只记模式不记地址/前缀。
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
  `client.json` 的 `active_subscription_id`，拒绝停用/不存在项，停用或删除生效项自动
  清除）、查看最后一次成功刷新原文（只读弹窗，无原文时明确提示先刷新）与带确认的
  删除；行内显示绑定的覆写模板名，模板被外部删除等导致引用失效时显示真实错误提示。
  联网仅在新增/刷新时按需发生：已有配置原样保留并遵守已保存的请求代理策略
  （`request_proxy_mode`/`request_proxy_url`/`github_proxy_prefix`），新装（无
  `client.json`/`app.toml`）才设直连、不读取系统代理（与 GPUI 为新配置设定的策略
  一致）；请求在后台线程执行，不启动核心、系统代理或 TUN。清单/配置的读写与覆写
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
- 订阅、请求设置、覆写模板与日志共用同一条数据根租约与按路径缓存：页面来回切换不重开缓冲，
  第二进程仍被拒绝。订阅清单（`subscriptions.json`）、覆写模板（`profiles.json`）与请求设置
  （`client.json`）的所有读写共用数据根上的同一把业务锁：校验引用与落盘在同一临界区内完成，
  不会出现“校验无引用后另一线程完成绑定”的竞争；锁不跨网络 await。
- 订阅原文与节点缓存在数据根 `subscription_cache/`（共享存储层布局）；当前生效订阅
  与请求设置持久化在本应用自己的 `client.json`（`active_subscription_id` 与
  `request_proxy_mode` / `request_proxy_url` / `github_proxy_prefix`），不触碰其他应用的配置。

```sh
cargo test --locked      # 全部测试：单元/数据根准入（src）+ 纯 Rust headless（tests/）
cargo run                # 本机原生后端直接运行 Rust 根组件
```

测试不依赖 QuickJS/TSX：headless 场景在 `tests/` 下用 `nsh_testing::Headless`
（内存 storage 后端预置每场景独立临时数据根，1100×800 桌面视口）直接驱动 Rust 根组件；
日志场景经真实 tracing 采集路径验证内存/磁盘/导出（保存对话框由 headless 注入应答），
订阅刷新场景经 `tests/common` 的本线程最小 HTTP 服务器（127.0.0.1 随机端口）真实联网，
断言实际请求路径与 User-Agent，不依赖互联网。请求设置场景复用同一服务器做正向代理：
页面保存 Custom 代理后触发订阅刷新，代理以绝对形式收到不可达目标
（`192.0.0.0/24` 文档地址）的完整请求，证明请求确实走新代理；Core 未启动负例把已保存
mixed_port 指向本机闲置端口，断言刷新真实连接拒绝、错误落盘，不等待超时。覆写模板场景
不联网：远程覆写地址只保存不下载；种子数据场景先启动一次应用初始化数据根（写入归属
标记）再预置业务文件，与生产准入一致。

`nsh.json` 命名根组件，`src/lib.rs` 是 UI，`theme.json` 沿用 `apps/nsh` 的浅色/深色
令牌。NSH 生成的文件在 `.nsh/`，Cargo 构建目录为 `target/`。
