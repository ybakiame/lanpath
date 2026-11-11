# NSH 跨平台验证（2026-11-11）

## 范围与工具

基线为 `01c930b`，新分支为 `nsh-validation`。`apps/nsh` 使用纯 Rust 和 NSH 0.5.0，点击按钮调用
`lanpath-client::parse_subscription_body` 与 `node_tags_from_nodes`，解析固定的两个测试节点。
不发起网络请求，不读取个人订阅，不启动核心、系统代理或 VPN。

NSH Linux/Windows 发行归档均通过上游 SHA-256 校验，CLI 报告 `nsh 0.5.0 (f4d68f7)`。临时下载、
生成 runner、APK、截图和无障碍树不入库。早期误下载的 0.4.0 工具已删除。

## 环境调整

`nix/desktop.nix` 增加 GTK4、GStreamer/base/good，保持 `devenv.lock` 不变。`lanpath-rust-targets`
补齐 `aarch64-linux-android`、`armv7-linux-androideabi` 和 `x86_64-linux-android`。这些变化用于
NSH 构建，不改变现有应用发布或签名流程。

## 共享逻辑与 headless

```sh
devenv shell -- target/nsh-tools/v0.5.0/nsh/bin/nsh test apps/nsh
```

结果：1 项通过、0 失败；共享解析返回 `ClashYaml`、两个中文节点和零警告。首次完整构建较慢，
本轮结果不用于性能对比。

## Windows

在 Windows 本地源码副本使用既有 Rust 1.98.0/MSVC 工具链执行 locked check/build，均通过。
真实 Win32 窗口启动后，经 UI Automation 调用“解析共享订阅”，文本角色与结果为：

```text
解析成功：2 个节点
格式：ClashYaml
警告：0 条

东京测试节点
新加坡测试节点
```

截图确认中文与结果正确。直接原生 runner 最初未自动读取 `nsh.json` 的主题，随后通过 `Root::tokens`
显式应用 `theme.json` 并重新构建、启动和截图确认。窗口正常关闭。

本轮未验证 DWM 亚克力、代理守护、TUN、单实例、安装更新或生产数据迁移。

## Android

```sh
devenv shell -- target/nsh-tools/v0.5.0/nsh/bin/nsh component add android
devenv shell -- target/nsh-tools/v0.5.0/nsh/bin/nsh package apps/nsh --target android
```

首轮发现缺少 armv7 Rust target，补齐后打包通过。APK 包含 arm64-v8a 与 armeabi-v7a 的
`libnsh.so`，包名 `com.ybakiame.lanpath.nsh`，版本 0.1.0，minSdk 26/targetSdk 35；apksigner
确认 v2/v3 调试签名有效。没有生成发布签名或替换发布身份。

经用户明确许可，在已连接的 Xiaomi 2112123AC / Android 13 设备上安装并启动，随后点击按钮。
实际截图和 UI 树均显示与 Windows 相同的两个节点、格式和警告结果。设备上的 MIUI
`theme_compatibility.xml` 缺失警告来自 `uiautomator dump`，不影响解析结果。

当前 APK 只有 NSH 库，没有现有 Go/libbox AAR，也未启动 VPN。本轮证明 NSH 页面与共享 Rust
解析可在真机运行，不证明 VPN 桥已可用。

## 下一轮

1. 优先解决纯 Rust NSH 与 Kotlin VpnService/Go AAR 的打包、授权、生命周期和真实代理流量。
2. 验证 Windows 平台能力适配，并做同负载资源测量。
3. 整理 `lanpath-client` 的脚本/MITM feature 边界；当前验证为绕开既有默认关闭编译错误而启用默认特性。
4. 保持 Tauri/GPUI 可用，只有在完整门禁通过后才讨论删除与生产切换。
