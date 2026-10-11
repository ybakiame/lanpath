# 主题二级页与纯黑背景验证（2026-10-11）

## 实现与偏好

设置主页通过主题摘要进入 `/settings/theme`。移动端删除 iOS/Material 选择与其状态和样式分支，
以编译期平台投影的 `data-platform` 呈现统一外观。旧风格存储键停止读取，未删除或迁移用户数据。

主题只有浅色、深色、跟随系统三种。纯黑背景是深色外观的独立布尔偏好，保存在
`lanpath-ui-oled-black`；浅色时开关禁用但偏好保留，跟随系统进入深色时应用。
首帧脚本与 React 使用相同存储键和生效条件：实际主题为深色且开关打开才设置纯黑标记。
背景、卡片、弹层、首页连接卡及导航使用纯黑表面，移除页面渐变，卡片用细边界分层。

## 验证

- 在项目 Nix 环境中运行 lanpath-app verify（桌面/Android 前端 build、lint、format）、
  @lanpath/ui verify 与 @lanpath/client-core verify；保留既有 Compiler 与 chunk 提示。
- 375×812 浏览器 Android mode mock：设置主页摘要入口、二级页返回、三个主题选项，
  无 iOS/Material 选项、无独立纯黑主题；纯黑背景开关有可访问名称。
- 深色下打开纯黑：主题存储仍为 dark，开关单独保存为 true，背景令牌为 #000，页面渐变为 none。
  刷新后深色与开关状态恢复正确。
- 切到浅色：移除纯黑 DOM 标记，恢复浅色表面，纯黑偏好仍为 true。
- 跟随系统：浏览器模拟系统 dark 后应用纯黑，模拟 light 后恢复浅色。
- 移动主题纯黑页面 axe WCAG A/AA 检查：0 violations、0 incomplete。
- 桌面 1440×900 主题页保持侧栏，没有返回箭头；主题路由下设置导航持续选中。
- 原有 RuleEditSheet 为 446 行，本轮仅更换图标，不新增职责或拆分表单。

## 边界

浏览器使用 browser-debug mock；没有安装 APK，未验证真实 Android 系统栏、VPN、输入法、
OLED 功耗与设备表现。纯黑背景的省电效果没有进行实机测量。
