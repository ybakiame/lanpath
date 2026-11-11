# NSH Desktop 首个切片验证（2026-11-11）

## 范围

`apps/desktop` 是纯 Rust NSH 0.5.0 的 NSH Windows 迁移目标首个切片：桌面窗口壳（侧栏导航 +
主内容区）只实现“订阅验证”页和“外观”页，无任何未实现占位页面。应用身份
`com.ybakiame.lanpath.desktop.nsh`，独立 Cargo 项目，不读取/迁移 Tauri/GPUI/apps/nsh 数据；
不启动核心、网络请求、系统代理或 VPN。apps/gpui 在迁移期内保留为 GPUI 对照。

- 订阅验证页调用共享 `lanpath-client::parse_subscription_body` 与 `node_tags_from_nodes`
  解析与 apps/nsh 相同的固定测试订阅。
- 外观页提供浅色/深色/跟随系统三个真实选项：选中即调用 `set_color_scheme`，选择经
  NSH storage 持久化，启动时恢复；`当前生效` 行经 `dark()` 环境值反映解析结果。

## 验证

```sh
devenv shell -- cargo fmt --manifest-path apps/desktop/Cargo.toml
devenv shell -- target/nsh-tools/v0.5.0/nsh/bin/nsh test apps/desktop
devenv shell -- cargo check --manifest-path apps/desktop/Cargo.toml --locked
```

- fmt：通过，无 diff。
- `nsh test apps/desktop`：4 项 headless 测试全部通过（共享订阅解析、两页导航与无占位页、
  浅色/深色/跟随系统的真实切换与持久化、storage 预置选择的恢复）。
- `cargo check --locked`：通过（首次 cargo check 生成 Cargo.lock 后执行）。

## 边界

已完成主机 headless 和 Windows 原生窗口验收：真实 Win32 程序启动后，共享解析、侧栏导航和
深色切换均正确，窗口正常关闭。未验证 DWM 亚克力、代理守护、TUN、安装更新或生产数据迁移。
主题令牌沿用 apps/nsh 的 theme.json。
