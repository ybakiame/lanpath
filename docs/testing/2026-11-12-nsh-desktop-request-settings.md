# NSH Desktop 请求设置切片验证（2026-11-12）

## 范围

`apps/desktop` 第五个切片：从 apps/gpui 迁移请求设置（`business/request_settings.rs`、
`request_settings_view`），适配 NSH 桌面独立数据根与纯 Rust NSH 组件。apps/gpui 仅作迁移
来源、未修改；apps/nsh 不动；不启动核心、系统代理或 TUN，不读取其他应用数据。
只做请求设置：不含端口、系统代理、TUN、核心与 GitHub 探针。

- 业务（`src/request_settings.rs`）：`RequestSettingsDocument{mode, custom_url,
  github_prefix}`（凭据刻意不实现 Debug）。读取/保存走共享 `load_for_requests`，
  `data_dir` 固定本根；新装（无 `client.json`/`app.toml`）直连 `None`，已有配置损坏
  返回真实错误、绝不重写为默认；读取不写盘。保存时在同一业务锁内重新加载最新
  `ClientConfig`，只改 `request_proxy_mode`/`request_proxy_url`/`github_proxy_prefix`
  三个字段，再经共享 `validate_request_proxy` 校验后落盘——打开页面后发生的 active、
  system/tun、MITM、端口等无关修改不被草稿覆盖；非法地址（scheme 非 http/https/
  socks5/socks5h、带 path/query/fragment、端口 0）拒绝且不写盘，错误为共享层固定文案，
  不回显用户名/密码/token/完整 URL。非自定义模式同样保存 `custom_url` 草稿供稍后切回
  （与 GPUI 一致）；GitHub 前缀保存值原样保留，使用时由共享 `apply_github_proxy_prefix`
  修剪拼接（页面不私自重写 URL、不做探针）。日志只记模式，不记地址/前缀。
- 共享入口：订阅刷新原先私有的 `request_config`（新装直连钉住）迁移为
  `request_settings::load_request_config` 并共享给 `Subscriptions`（refresh/active/
  set_active/clear_active），订阅与请求设置读取同一策略，无两处新装默认分叉。
- UI（`src/request_settings_view.rs`）：侧栏新增“请求设置”页。四种模式单选；
  自定义 URL 输入在非 Custom 模式只读但不清空；GitHub 前缀输入；保存/重置按钮；
  已保存摘要（模式 + 镜像是否已配置，不回显地址内容）；脏状态与错误/成功提示。
  保存成功后草稿回写为已保存值（不会保存后仍显示旧草稿）；重置有未保存修改需确认，
  放弃后恢复已保存草稿，非法保存保留草稿。页面明确说明：设置用于下一次订阅刷新与
  远程资源请求；核心代理只指向已保存 mixed_port，本切片不启动核心，核心未运行时请求
  真实失败；不做连接测试、不提供系统代理/TUN 开关、不显示假成功。

## 验证

```sh
devenv shell -- bash -lc 'cargo fmt --manifest-path apps/desktop/Cargo.toml && \
  cargo test --manifest-path apps/desktop/Cargo.toml --locked && \
  cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings && \
  cargo build --manifest-path apps/desktop/Cargo.toml --locked'
```

- fmt：通过，无 diff。
- `cargo test --locked`：117 项通过、1 项 ignored（跨进程锁争夺子进程用例，由父用例以
  子进程驱动并通过）。其中 `src/` 70 项（数据根准入 20、请求设置业务 7、订阅业务 20、
  覆写模板业务 11、日志缓冲 3、采集 2、磁盘写入 2、日志服务 5），`tests/` headless
  47 项（请求设置页 8、订阅页 18、覆写页 6、日志页 12、外观与导航 3）。
- 业务覆盖：新装直连/data_dir 固定/读取不写盘；四模式重开（同根共享句柄）且共享消费端
  `load_for_requests` 读回同一策略、GitHub 前缀按共享语义拼接（保存值原样含空白）；
  非自定义模式保留自定义地址草稿；保存合并最新无关字段（active/rule_mode/MITM/system/
  tun/IPv6/端口/Clash API 逐项对齐）；非法自定义地址六例（path/ftp/端口 0/query/
  fragment/非 URL）错误不含凭据与完整 URL、旧文件字节不变、已保存文档不变；读取两次
  文件字节不变；损坏配置读与保存都返回真实错误且文件不被重写为默认。
- headless 覆盖（`nsh_testing::Headless`，每场景独立临时数据根 + 桌面视口）：导航与
  作用域说明；新装默认直连且仅打开页面不写 `client.json`；Custom + 地址 + 前缀保存后
  重开应用输入框与磁盘配置一致；切模式不清空地址草稿并随其他模式保存；非法输入错误
  脱敏（屏幕任何文本不含用户名/密码/完整 URL）、草稿保留、不写盘；重置双确认（继续
  编辑保留草稿/放弃修改恢复已保存）。
- 真实联网（本线程 `std::net` 127.0.0.1 HTTP server，不依赖互联网）：
  - 正向代理：页面保存 Custom 代理（服务器自身地址）后新增订阅指向不可达的
    `http://192.0.2.99/proxied.yaml`（文档保留地址，测试机不可达），自动刷新成功且
    代理服务器以绝对形式收到完整目标 URL 恰好一次——请求确实走新代理，不只测 helper。
  - Core 未启动负例：预置 `mixed_port` 指向本机闲置端口，页面切 Core 保存后刷新立即
    连接拒绝（两次重试均瞬时失败，无 30 秒超时），真实错误上屏并落盘到订阅记录，
    不伪报成功。
- clippy `--all-targets -D warnings`、`cargo build --locked`：通过；未新增/变更 Cargo 依赖。

## 环境备注

仓库 rust-toolchain 固定 1.98.0；验证一律在 `devenv shell --` 内执行（Nix rustup +
一致 glibc）。headless 的 `wait_for` 裸字符串按整节点文本精确匹配，子串等待需用
`text:/…/` 选择器。

## Windows 原生验收

在 Windows 本地副本使用 Rust 1.98.0 / MSVC 执行 `cargo build --locked` 成功。以独立
临时数据根和 UI Automation 验收真实窗口：

- 初次打开显示直连且不落 `client.json`；选择自定义代理、保存带测试凭据的本地代理
  URL 与 GitHub 前缀，保存后草稿和配置一致。
- 非法带路径且含测试凭据的地址被拒绝，错误不含凭据、路径或端口；重置先确认，
  “继续编辑”保留草稿，放弃后恢复已保存代理。
- 新增指向保留测试地址 `192.0.2.99` 的订阅，请求实际到达页面配置的 127.0.0.1
  `HttpListener`，Host/路径保持原目标；返回一个真实节点后页面显示刷新成功。
- 正常退出后扫描本应用磁盘日志，代理用户名/密码和 GitHub 前缀均未出现。

Core 未启动负例由 headless 使用明确的闲置端口验证，Windows 未借用用户可能正在
使用的默认代理端口。headless 另覆盖四模式重开与保存合并；本切片没有系统保存对话框。
