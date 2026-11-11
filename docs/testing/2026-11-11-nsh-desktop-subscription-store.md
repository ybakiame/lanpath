# NSH Desktop 订阅存储切片验证（2026-11-11）

## 范围

`apps/desktop` 第二个切片：订阅页从“固定订阅解析验证”升级为真实订阅列表与持久化。
复用共享 `lanpath-client::SubscriptionStore` / `Subscription` / `SubFormat`，数据写入 NSH
桌面自己的隔离数据根，不读取/迁移 Tauri/GPUI/apps/nsh 数据；仍不启动核心、网络请求、
系统代理或 VPN。apps/gpui 保持 GPUI 对照，apps/nsh 不变。

- 数据根：从 apps/gpui 迁移并适配为 `.lanpath-desktop-nsh` 身份（所有权标记 +
  进程内文件锁租约 + 数据根 UUID 身份 + 生产/便携祖先、符号链接、硬链接共享文件的准入
  检查），链接/所有权/租约/路径约束未削弱；Windows 私有 ACL 目录逻辑一并迁移
  （`windows_access`/`windows_file`，cfg(windows)）。不提供 GPUI 的便携模式。
- 根定位：默认 `%LOCALAPPDATA%\lanpath-desktop-nsh` / `~/.local/share/lanpath-desktop-nsh`；
  `LANPATH_DESKTOP_DATA_DIR` 环境变量优先，NSH storage 键 `lanpath_desktop_nsh.data_dir`
  次之（宿主/测试注入，同一套准入）。显式路径非法即错误，不回退。
- 订阅页：启动加载真实订阅（名称、启用状态、节点数、错误、格式），名称 + URL 新增表单
  （共享 `normalize_resource_url` + `validate_subscription_url` 校验；失败保留输入并显示
  错误，不覆盖已有数据），启用切换、删除（确认对话框）。底部保留固定订阅共享解析验证。
- UI 全部为 NSH 原生 input/button/dialog/switch/signal，无 TSX 产品 UI、无未实现入口。
- 测试为纯 Rust：`src/` 单元与数据根准入测试 + `tests/` 下 `nsh_testing::Headless`
  场景（内存 storage 后端预置每场景独立临时数据根，1100×800 桌面视口）。仓库目标移除
  Tauri/Husky/Node 后 apps/desktop 不保留任何 TSX 测试文件；2026-11-12 起原
  App.test.tsx 的 10 项 headless 场景已一一等价迁移为 `tests/` 下 10 项 Rust 用例
  （订阅页 7 项、导航与外观 3 项，共用 tests/common harness），行为覆盖无删减，
  App.test.tsx/tsconfig.json 移除。

## 验证

```sh
devenv shell -- cargo fmt --manifest-path apps/desktop/Cargo.toml -- --check
devenv shell -- cargo test --manifest-path apps/desktop/Cargo.toml --locked
devenv shell -- cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets -- -D warnings
devenv shell -- cargo check --manifest-path apps/desktop/Cargo.toml --locked
```

- fmt：通过，无 diff。
- `cargo test --locked`：35 项通过、1 项 ignored（跨进程锁争夺子进程用例，由父用例
  以子进程驱动并通过）。其中 `src/` 25 项（数据根准入 20、订阅业务 5），`tests/`
  headless 10 项。
- headless 覆盖（`nsh_testing::Headless`，每场景独立临时数据根 + 桌面视口）：
  共享解析验证；空列表；真实新增与表单清空；同一数据根重开持久化；无效 URL 与
  空名称不覆盖（含输入值断言）；启用切换；删除确认与持久化；两页导航；外观
  浅色/深色/跟随系统真实切换、storage 持久化与系统外观联动；storage 预置外观恢复。
- clippy `--all-targets -D warnings`、`cargo check --locked`：通过。

## 边界

主机 headless 与 Rust 层验证通过后，Windows 原生 Debug 程序也已在独立数据根完成真实闭环：
新增订阅、正常关闭、重开后名称恢复、删除确认及最终关闭均通过。长列表区域使用独立滚动容器。
Windows 私有 ACL 路径已在真实 Windows 构建与数据根写入中执行，但未做全部权限负例复测。

订阅只落 `subscriptions.json` 元数据：节点数/格式/错误来自共享存储字段，新增时仅校验不嗅探正文
（无网络）；订阅内容缓存、刷新、编辑不在本切片。数据根锁为进程内租约：同进程多开
（headless 测试）经路径缓存共享，第二进程会被拒绝。
