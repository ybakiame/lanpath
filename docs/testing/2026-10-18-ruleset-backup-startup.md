# 规则集备份与启动反馈回归（2026-10-18）

## 根因与变更

v1 快照白名单未包含 rulesets/，只迁移了定义和时间戳，恢复后需串行联网补下载。
此前补下载没有按实际引用筛选；更新时间也可能令缺文件的手动更新错误跳过。
启动持有 client mutex，原运行状态查询无法及时显示配置准备进度。

v2 备份纳入规则集文件并保持字节原样；兼容 v1，恢复前副本同样包含规则集。
旧事务 journal 按原根目录列表回滚，新事务显式记录根目录，覆盖规则集恢复中断。
启动仅补齐启用路由/DNS 引用的缺失远程文件，手动内容可从配置重建。
新独立阶段查询在启动期间每 500 ms 由 Query 读取，双端展示实际阶段，结束后隐藏。

## 自动验证

在本仓 `nix develop --no-pure-eval` 执行：

- `lanpath-verify-rust`（根 build、fmt/clippy/test、独立壳 host clippy 与 Android target check）。
- `lanpath-verify-frontend`（应用桌面及 Android 构建、client-core、ui 的 verify）。
- `lanpath-test-tools`（运行缓存、Portable、发布工具回归）。
- `lanpath-build-android-debug`（原生 Rust/Kotlin/资源与 APK/AAB 构建；本轮无 Go 修改）。

主库 553 项、命令层 53 项测试通过；根构建、fmt/clippy、独立壳 host/Android check、三个前端包 verify 与 10 项工具回归通过。Android debug APK/AAB 及 Kotlin/资源编译通过。

关键回归覆盖：v2 规则集二进制/JSON 加密往返及恢复、v1 读取、旧 journal 不误删新目录、
v2 规则集回滚、已有缓存跳过、缺缓存即使时间较新也下载、只下载启用路由/DNS 引用、
手动文件重建不覆盖已有文件、阶段 guard 退出时清除。

## 浏览器交互（mock）

agent-browser 配合独立 Vite Android/desktop 模式，使用 example.invalid 演示订阅与挂起的启动 Promise：

- 390×844 移动浅色、375×812 深色与 1280×900 桌面展示配置合成/规则集下载阶段，无横向溢出。
- 移动端切设置再回首页后，当前阶段继续更新，启动按钮禁用，启动调用仍仅 1 次。
- 失败后隐藏阶段并允许重试；重试成功后显示已连接并隐藏阶段。
- 桌面合成阶段可见，失败后隐藏阶段。
- 图标使用 Lucide，装饰图标 aria-hidden；反馈使用 output/polite 语义，旋转受 motion-safe 限制。

截图保留于本机临时目录，不提交生成图片或个人数据。

## 设备边界

未安装/重启用户设备。浏览器不验证真实 IPC、Android VPN、网络吞吐或 Windows→Android 实际恢复耗时。
设备应复测：源客户端升级后重新导出含规则集文件的备份，Android 导入重启后首次启动；
旧 v1 导入首次联网下载且后续缓存命中；断网及规则集源不可达时的阶段显示与既有降级行为。
不承诺旧备份离线补齐远程文件；外部路径文件、源设备未下载的资源仍需单独准备。
