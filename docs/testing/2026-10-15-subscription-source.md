# 订阅原文、列表与仪表盘覆写验证

日期：2026-10-15。

## 本地检查

在本仓 `nix develop --accept-flake-config --no-pure-eval` 中执行：

- `lanpath-verify-frontend`：应用 desktop/Android 构建、类型、lint、format；client-core/UI verify。
- `lanpath-verify-rust`：根 workspace build/fmt/clippy/test，独立桌面壳 clippy、Android target check。
- `lanpath-test-tools`：9 项运行缓存、Portable、发布清单与签名工具回归。
- `lanpath-build-android-debug`：生成 arm64 debug APK/AAB，Kotlin/资源编译成功。

新增 Rust 回归覆盖真实 HTTP 响应的 BOM/CRLF 保留、拒绝非 UTF-8、YAML/JSON/base64 原样缓存、
转换缓存独立、历史缓存缺失、URL 变更/删除清理，以及刷新不覆盖同期模板关联、名称、启用状态。
失败解析不替换上次成功原文。构建中的既有 React Compiler finally 与 chunk 大小提示不阻塞检查。

## 浏览器交互（mock）

使用 agent-browser 独立会话与无真实凭据的 IPC fixture：

- desktop 1280×577、760×560，Android 模式 390×844：订阅卡片和弹层无横向溢出。
- 原文完整保留 YAML 注释和 CRLF；点击导出只发送指定订阅 id，成功展示下载路径。
- 原文不存在时提示刷新，导出按钮禁用；原文弹层关闭后销毁查询缓存。
- 仪表盘选择模板调用专用绑定命令；保存期间启动按钮禁用，完成后展示新模板。
- 订阅表单不再包含覆写选项；移动端能从仪表盘进入模板管理并返回。
- 最终原文弹层 axe WCAG 2 A/AA：0 violations，背景渐变与模态焦点守卫共 2 类 incomplete，仍需人工确认。
- 仪表盘扫描发现 1 类既有对比度问题：空配置启动提示的 warning 文本在浅色背景不足 4.5:1；本次不据此声称完整无障碍认证。

## 设备与发布边界

本地未在 Windows/Android 真机执行 Downloads 文件落盘、安装覆盖、VPN/TUN 或系统代理验收。
Android 调试签名与公开 release 签名不同，不将 debug APK 当作 beta 升级包。
下一 beta 继续采用手动发布页更新；签名清单验证不能替代真实安装升级。
