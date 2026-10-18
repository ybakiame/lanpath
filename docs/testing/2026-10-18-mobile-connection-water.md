# 移动端连接卡片水波与 0.1.4-beta.1 验证

## 范围

移动端 StatusCard 的装饰层，不改变启停 IPC、VPN 授权、连接查询或流量语义。
准备/失败时静止，连接中和已连接时缓慢流动，停止时水面回落；触摸为单次扩散。
不接入传感器、不增加配置迁移或运行时依赖。`.reference/` 作为本地参考资料忽略。

## 已执行

- 本仓 Nix 环境下执行三包 `lanpath-verify-frontend`：双端应用构建、lint/format 及共享包验证通过。
- `lanpath-test-tools`：10 项通过。
- `lanpath-verify-rust`：根 workspace build/fmt/clippy/test、独立桌面壳 host clippy 和 Android target check 通过。
- 版本和锁文件通过 Bun/Cargo 命令同步，无外部依赖升级。
- agent-browser 使用 Android mode 浏览器 mock：准备态水面静止；模拟真实状态查询返回 running 后，标题与主操作切为已连接/停止，三层水面运行。
- 临时 mock 控制启停 Promise：连接中/停止中阶段与按钮 busy 一致，完成后恢复对应状态，未重复触发实际原生请求。
- 模拟 pointerdown 后只有一个涟漪动画；动画不阻挡滚动、按钮或辅助技术，不将装饰添加到可访问树。
- 模拟系统减少动态效果：active 为 false，持续动画数量为 0，涟漪关闭；恢复设置后可继续。
- 将卡片滚出滚动容器：active 为 false，三层动画均 paused；标签页进入后台/返回前台时，visibilitychange 记录对应暂停/恢复。
- 检查浅色和深色纯黑呈现；320px 宽度无横向溢出。截图及原始日志仅临时保存，不入库。

## 边界

浏览器状态由临时 mock 提供，不代表真实代理已启动。仅检查到 ADB 设备及已有应用版本，未安装、清除数据或改变设备代理。
真机触摸/滚动手感、帧率、长时间耗电、VPN 授权与安装升级尚未验收。签名发行产物和同提交 CI 以远程工作流结果为准。
