# NSH Desktop 覆写模板切片验证（2026-11-12）

## 范围

`apps/desktop` 第五个切片：从 apps/gpui 迁移覆写模板管理（`business/profiles.rs`、
`profiles_view`，交互参考不照搬框架），并给订阅编辑加上覆写模板绑定。apps/gpui
仅作迁移来源、未修改；apps/nsh 不动；仍不启动核心、系统代理或 TUN，不读取其他
应用数据；远程覆写地址本切片只保存供后续配置合成使用——保存/打开页面不下载、
不预览、不显示“已生效”，无远程预览/config preview/核心启停。

- 覆写页：真实模板列表（共享 `lanpath-client::ProfileStoreV2`，本数据根
  `profiles.json`）；新建/编辑共用一个弹窗表单，删除需确认。名称 trim 必填且
  除自身外唯一；YAML/JS 本地覆写真实调用共享 `validate_yaml_override` /
  `validate_js_override`（空允许），用 NSH 原生多行输入（`Input` 的 `multiline`，
  UITextView 式、保留换行）编辑，不塞进单行框；两个远程 URL 可选、trim 规范化
  并共享 `validate_remote_url` 校验。保存失败保留草稿并显示真实错误，取消有
  未保存修改需确认（继续编辑保留草稿）。
- 删除语义：被任一订阅 `profile_id` 引用的模板在同一业务锁内拒绝删除并显示
  真实错误（不静默解除引用），解除绑定后才可删。
- 订阅绑定：编辑弹窗以单选列表绑定“无覆写”或一个现存模板（多个模板同样清晰
  可选）；保存时业务层校验模板引用存在，`subscription.profile_id` 持久化；
  新增订阅仍 `None`；刷新/启停/设置生效/改名均不改变绑定；行内显示绑定模板名；
  模板被外部删除或 `profiles.json` 损坏导致引用失效时行内显示真实错误提示，
  不静默改绑。
- 并发：订阅业务原有的自身写锁迁移到共享 `Arc<DataRoot>` 上的业务锁
  （`DataRoot::business_guard`，crate 内使用），订阅与覆写模板的清单/配置操作
  共用同一把锁：删除前的无引用校验与落盘、绑定前的存在校验与落盘都在同一临界区，
  不存在“校验无引用后另一线程完成绑定”的竞争；锁不跨网络 await（刷新在请求
  前后分别短暂取锁）。`open_root` 按路径缓存数据根，Profiles 服务无自身状态、
  无独立锁，重开只是再取共享句柄。
- 日志：只记名称/结果/节点数/模板 id 等，不记 YAML/JS 正文、远程覆写 URL 或
  订阅 URL。

## 验证

```sh
devenv shell -- bash -lc 'cargo fmt --manifest-path apps/desktop/Cargo.toml && \
  cargo test --manifest-path apps/desktop/Cargo.toml --locked && \
  cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings && \
  cargo build --manifest-path apps/desktop/Cargo.toml --locked'
```

- `cargo test --locked`：102 项通过、1 项 ignored（跨进程锁争夺子进程用例，
  由父用例以子进程驱动并通过）。其中 `src/` 63 项（数据根准入 20、订阅业务 20
  含绑定校验/绑定保留/本地 HTTP server 刷新、覆写模板业务 11 项含同根并发不变量、
  日志缓冲 3、采集 2、磁盘写入 2、日志服务 5），`tests/` headless 39 项
  （订阅页 18、覆写页 6、日志页 12、外观与导航 3）。
- 业务覆盖：模板 CRUD 与重开恢复（名称 trim、URL 规范化、多行内容原样）、重名
  拒绝（编辑自身除外）、空/空白名称拒绝、非法 YAML（非 mapping）/JS（缺 main）/
  远程 URL（非 http(s)）拒绝且已有数据与磁盘不被覆盖；删除被绑定模板拒绝、
  解绑后删除成功且不残留引用；绑定校验（不存在模板与损坏 `profiles.json` 均真实
  报错，不落盘）；新增订阅无绑定；启停/生效/改名/刷新（本地真实 HTTP server）
  保留绑定；同根 6 线程并发绑定/删除后无悬空引用。
- headless 覆盖（`nsh_testing::Headless`，每场景独立临时数据根 + 1100×800 桌面
  视口）：侧栏导航到覆写页；多行 YAML/JS 输入保存、重开应用后编辑框回读内容与
  磁盘一致（换行保留）；编辑草稿取消双确认；重名/非法 YAML/JS/URL 错误上屏且
  磁盘不写入；两模板单选列表真实区分并绑定第二个模板、行内显示绑定名、跨重开
  持久化；删除确认 → 被引用拒绝（真实错误）→ 解绑 → 删除成功且磁盘清空；
  外部删除模板文件后行内显示“覆写模板引用失效”真实错误，不静默改绑。
  覆写场景不联网；种子数据场景先启动一次应用初始化数据根（归属标记）再预置
  业务文件，与生产准入一致。
- clippy `--all-targets -D warnings`、`cargo build --locked`：通过（exit 0）。

## 环境备注

仓库 rust-toolchain 固定 1.98.0；验证一律在 `devenv shell --` 内执行
（Nix rustup + 一致 glibc）。未新增依赖，apps/desktop/Cargo.lock 无变化。

## Windows 原生验收

在 Windows 本地副本使用 Rust 1.98.0 / MSVC 执行 `cargo build --locked` 成功。以独立
临时数据根启动真实应用，通过 UI Automation 完成以下闭环；订阅刷新仅访问测试进程
持有的 127.0.0.1 `HttpListener`，远程覆写 URL 仅保存，未发起对应请求：

- 创建两个模板，填写多行 YAML/JS 与带首尾空白的远程 YAML URL；保存后重进编辑框，
  ValuePattern 回读的换行和正文逐字一致，URL 已规范化。
- 本地订阅新增/刷新成功后，在两个模板选项中真实选择“原生模板”，列表显示绑定名。
  删除被绑定模板经确认后被拒绝，显示共享层真实错误。
- 将 JS 改为非法内容，错误上屏且 `profiles.json` 未写入该内容；未保存取消先确认，
  “继续编辑”保留草稿，“放弃修改”不影响已保存模板。
- 回订阅页选择“无覆写”并保存，再删除模板成功，数量由 2 变 1；`WM_CLOSE` 正常退出。

headless 断言与上述 Windows 原生交互互补；本切片不涉及系统保存对话框或核心进程。
