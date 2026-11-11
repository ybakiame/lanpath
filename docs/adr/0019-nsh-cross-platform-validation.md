# ADR-0019：NSH 跨平台验证分支

- 状态：Proposed（仅验证，不是生产替换）
- 日期：2026-11-11
- 决策来源：用户要求以当前 GPUI 分支为 base，新建 NSH 分支快速验证和迭代。

## 背景

GPUI 桌面已复用共享 Rust 业务，但不提供当前所需的 Android 产品路径。NSH 0.5.0 提供纯 Rust
组件、Windows 原生后端和 Android 原生视图，需要验证它能否在不重写核心与 VPN 引擎的情况下
成为统一 UI 层。NSH 的组件和 signal 模型不是 GPUI Entity/Render 的直接兼容层。

## 决定

- 从 `feat/gpui-desktop-validation` 的 `01c930b` 创建 `nsh-validation`，不推送或改变生产分支。
- `apps/nsh` 是独立 Cargo/NSH 验证应用，使用纯 Rust，不引入 TSX 产品界面。
- 随后将原 `apps/desktop` 重命名为 `apps/gpui` 并保留为对照；新 `apps/desktop` 使用 NSH
  迁移 Windows 功能，具有独立身份、数据根和 Cargo 项目，不与 GPUI 双写或共享运行状态。
- NSH 固定为 0.5.0（CLI 提交 `f4d68f7`）；各验证应用使用各自的 `com.ybakiame.lanpath*.nsh`
  身份，不继承 Tauri/GPUI 的数据根、端口、更新渠道和签名身份。
- 首个切片只调用 `lanpath-client` 的共享订阅解析和节点标签生成；后续桌面切片依次加入真实
  持久化与业务页面，不在未实现时显示占位操作，不启动未经当前切片授权的系统效果。
- 保留现有 Kotlin VpnService 和 Go/libbox 作为后续 Android 集成边界；页面与解析成功不等于
  VPN 已接入。
- NSH 测试使用 Rust `nsh_testing::Headless` 或普通 Rust 测试，不保留 TSX 测试或 Node 工具链；
  现有生产 Husky 只在正式退役阶段统一处理。
- 为 NSH 构建补齐 GTK4/GStreamer 和 Android ABI targets，不升级 devenv 输入或既有工具版本。
- 本验证可在本机使用现有原生 Windows Rust/MSVC 工具链构建和运行 `apps/nsh` 与新
  `apps/desktop`；不安装或升级宿主组件，不扩展到发布路径。

## 已获证据

- NSH 测试实际调用共享解析并验证两个中文节点。
- Windows 原生 cargo check/build 和真实窗口交互通过，中文、主题、UIA 文本及结果正确。
- Android APK 在 Android 13 真机安装、启动并点击解析通过；APK 为 arm64-v8a/armeabi-v7a，
  使用独立调试签名。
- 新 desktop 已验证独立数据根、订阅新增/启停/删除、外观持久化和 Windows 重开恢复。
- 详细命令与边界见[跨平台验证](../testing/2026-11-11-nsh-validation.md)、
  [desktop 首个切片](../testing/2026-11-11-nsh-desktop-first-slice.md)与
  [订阅存储切片](../testing/2026-11-11-nsh-desktop-subscription-store.md)。

## 后续门禁

1. 证明纯 Rust NSH 与现有 Kotlin VpnService、Go AAR 的生命周期和打包可以组合，再开始完整页面迁移。
2. Windows 验证 DWM 亚克力、代理守护、TUN helper、单实例、数据迁移及签名更新，不把当前简单窗口
   当作这些平台能力已经兼容。
3. 每个业务切片同时验证真实持久化、错误恢复、Windows 与 Android 行为；不以页面可打开代替功能对等。
4. Tauri/GPUI 的退役和生产身份切换仍需独立 Accepted 决策及回退证据。本 ADR 不授权删除旧方案。

## 后果

本分支保留一个可删除的 NSH 验证目标，不改变现行发行实现。共享 `lanpath-client` 在
`default-features=false` 时存在既有脚本引用编译错误，本轮验证沿用默认特性；后续应整理 feature
边界，避免把桌面脚本/MITM 体积无意带入移动端。
