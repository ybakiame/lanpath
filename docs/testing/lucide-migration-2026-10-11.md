# Lucide 图标迁移验证（2026-10-11）

客户端页面与共享 UI 控件采用 `lucide-react` 具名图标；提示条、弹窗、选择器、复选框及加载指示同步迁移。
Bun 在根 workspace 管理两包的直接依赖与唯一锁文件，删除客户端 Heroicons 依赖。
品牌资产保持独立，装饰图标隐藏于无障碍树，图标按钮保留可访问名称。

项目 Nix 环境中 lanpath-app verify（双目标 build、lint、format）、@lanpath/ui verify 与
@lanpath/client-core verify 通过；保留既有 React Compiler 与大 chunk 提示。
源码和锁文件搜索确认没有 Heroicons 依赖与旧路径。浏览器 mock 验证移动设置/主题页与桌面
订阅、代理、统计、日志、DNS 页能正常加载图标；不代表真实设备验证。
