# NSH Desktop 配置合成预览切片验证（2026-11-12）

## 范围

`apps/desktop` 第六个切片：从 apps/gpui 迁移“配置合成预览”（`business/config.rs`
的 sing-box 路径：`build_config_report`/`panel_features`/`remove_dashboard_download`），
适配 NSH 桌面独立数据根与纯 Rust NSH 组件。apps/gpui 仅作迁移来源、未修改；
apps/nsh 不动；不启动核心、系统代理或 TUN，不读取其他应用数据。只做共享合成
预览：不做核心下载/安装/校验/启停，不做 Mihomo。

- 业务（`src/config_composer.rs`）：`ConfigComposer::compose()` 按已保存配置快照
  合成完整 sing-box JSON，返回 `ConfigPreview{json, node_count, warnings,
  missing_tags}`。合成语义与 GPUI 对齐：active 订阅必须存在且启用；优先本地节点
  缓存；无缓存时 Core 请求策略明确拒绝（“核心请求代理尚未就绪，且所选订阅没有
  可用节点缓存…”），其余模式用进入网络前固定的 `ClientConfig` 快照（业务锁内读出）
  经 `Subscriptions::refresh_with_config` 联网刷新一次——网络途中设置页的修改不改变
  本次策略；`refresh_with_config` 与 `refresh` 共享同一实现，写回仍按 id/URL/UA 防
  stale，业务锁不跨 await。已保存 `core_type=Mihomo` 明确报“暂不支持”，不走
  sing-box 错误路径。
- 覆写解析：关联 Profile 必须真实存在（失效引用真实报错）；远程 YAML/JS 经共享
  `resolve_remote_overrides_with_context` 与 `resource_storage::cache_path(root,
  "profile_cache")` 解析，非 Core 策略 `RemoteFetchContext::stopped(config)`、Core 用
  `cache_only()`，远程失败按共享降级并返回真实警告。配置切片经 `ConfigSlicesStore::load`
  （当前默认，不做切片编辑）；读取失败按 GPUI 回退共享默认值，并把真实警告展示给
  用户（共享存储层只记 tracing 警告，本切片把回退判定显式化）。
- 合成链：`build_core_config_v2` + `compose_singbox_config(mixed_port, None)`；
  `download_rule_sets=true` 走 GPUI 同路径 `prepare_missing_rule_sets_warn_only` +
  `prepare_builtin_rule_sets` + `materialize_rule_sets`/`existing`，缺失按共享降级并
  回报 missing tags；`inject_local_override_warn_only`、`apply_panel_features`
  （GPUI 字段，`data_dir` 为本根 `cache`，`normalized_rule_mode`/`dns_mode_from_slices`）
  后移除 `experimental.clash_api` 的 `external_ui`/`external_ui_download_url`，避免预览
  触发面板下载。日志只记节点数、警告数与结果，不记配置正文、节点凭据、远程 URL 或
  代理 URL。
- UI（`src/config_view.rs`）：侧栏新增“配置”页。固定文案“共享合成预览，未进行核心
  二进制校验 · 按当前生效订阅与已保存设置合成 sing-box JSON · 不启动核心/系统代理/
  TUN”，绝不显示“已验证/已启动”。点击“生成配置预览”后 busy 单飞（在途重复点击不
  启动第二个合成），合成在 `background`（tokio 线程）执行、Signal 留在 UI 线程
  `spawn` 任务内回写（不跨 Send）；只读滚动区域展示完整 pretty JSON（大内容不卡死、
  非单行），上方为上次成功摘要（节点数/警告数/缺失规则集数）与警告列表，错误真实
  上屏。失败保留旧预览、不伪报新成功；新预览进行中保留旧预览并明确 busy。离开页面
  在途任务随 owner 取消，重进页面是全新状态，不永久 busy。

## 验证

```sh
devenv shell -- bash -lc 'cargo fmt --manifest-path apps/desktop/Cargo.toml && \
  cargo test --manifest-path apps/desktop/Cargo.toml --locked && \
  cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings && \
  cargo build --manifest-path apps/desktop/Cargo.toml --locked'
```

- fmt：通过，无 diff。
- `cargo test --locked`：135 项通过、1 项 ignored（跨进程锁争夺子进程用例，由父用例
  以子进程驱动并通过）。其中 `src/` 82 项（数据根准入 20、请求设置业务 7、订阅业务
  20、覆写模板业务 11、配置合成业务 12、日志缓冲 3、采集 2、磁盘写入 2、日志服务 5），
  `tests/` headless 53 项（配置页 6、请求设置页 8、订阅页 18、覆写页 6、日志页 12、
  外观与导航 3）。
- 业务覆盖（本线程 `std::net` 127.0.0.1 真实 HTTP 服务器，不依赖互联网）：
  - 前置拒绝：无 active / active 目标不存在 / 已停用 / 关联模板被删除，各自真实
    错误文案；`core_type=Mihomo` 明示“暂不支持 Mihomo”且不走 sing-box 路径。
  - 缓存语义：有缓存 + Core 策略合成成功且服务器零请求（离线）；无缓存 + Core
    策略拒绝且服务器零请求。
  - 快照语义：无缓存 + Custom 策略，订阅响应延迟 500ms，途中把已保存策略改成 Core，
    本次合成仍按快照成功（若途中重读配置则 refresh 与 RemoteFetchContext 都会失败）。
  - 覆写：远程 YAML/JS（经自定义代理从本地服务器取到，URL 以 absolute-form 到达）
    与本地 YAML/JS 按“远程 YAML → 本地 YAML → 远程 JS → 本地 JS”顺序真实作用于
    JSON（覆盖层级与 JS 链式标记断言）；远程 404 且无缓存时共享降级警告上屏。
  - 面板字段：`clash_api_enabled` 默认 true，注入后 `external_ui`/
    `external_ui_download_url` 被移除、`external_controller` 保留。
  - 切片回退：`config_slices.json` 损坏时合成成功、默认切片生效、用户可见警告。
- headless 覆盖（`nsh_testing::Headless`，每场景独立临时数据根 + 桌面视口）：页面
  打开即显示“共享合成预览，未进行核心二进制校验”与空闲态；合成后完整 pretty JSON
  （节点 tag、mixed_port）、摘要与“内置规则集缺失”警告上屏、JSON 不含 external_ui；
  busy 单飞（在途重复点击只产生一次订阅请求）且完成后恢复可再生成；失败保留旧预览
  并显示真实错误；合成中途切走再切回不永久 busy，可重新开始并成功；Mihomo 配置
  明示暂不支持且不伪报成功。
- 规则集边界：离线（Core/cache_only）与经本地代理 404（None/Custom）两条路径都只
  覆盖“缺失按共享降级回报”分支；规则集真实镜像下载成功分支未覆盖（不访问规则集
  公网，由共享层既有测试承担）。
- clippy `--all-targets -D warnings`、`cargo build --locked`：通过；`apps/desktop`
  新增直接依赖 `serde_json = "1"`（lanpath-client 公共 API 的 `serde_json::Value`
  需要命名），`Cargo.lock` 仅增加该直接依赖边，版本不变。

## 环境备注

仓库 rust-toolchain 固定 1.98.0；验证一律在 `devenv shell --` 内执行（Nix rustup +
一致 glibc）。headless 的 `wait_for` 裸字符串按整节点文本精确匹配，子串等待需用
`text:/…/` 选择器。预置种子的场景先用应用初始化数据根再写 `client.json`，否则准入
会把带生产配置文件的未归属目录当作生产根拒绝。

## Windows 原生验收

在 Windows 本地副本使用 Rust 1.98.0 / MSVC 执行 `cargo build --locked` 成功。以独立
临时数据根和 UI Automation 验收真实窗口，网络仅经过受控 127.0.0.1 `HttpListener`：

- 保存自定义代理，新增订阅后收到保留测试地址的 absolute-form 请求；远程 YAML 也从
  本地服务取得。关联覆写并设为生效后生成预览，完整 JSON 含真实节点、mixed_port 和
  本地 JS 写入的日志级别；摘要与缺失规则集警告可见，不含 external_ui 字段。
- 删除本测试根的模板文件后再次生成，显示“覆写模板不存在”的真实错误，旧 JSON
  保持不变。页面固定标注“未进行核心二进制校验”，没有启动核心/系统代理/TUN。
- `WM_CLOSE` 正常退出；扫描磁盘日志，配置正文、节点密码、订阅/远程/代理地址均未出现。

busy 单飞、离开页面恢复、Mihomo 拒绝与 Core 无缓存拒绝由 headless 覆盖；Windows
未访问规则集公网，未进行核心二进制校验或核心启动。
