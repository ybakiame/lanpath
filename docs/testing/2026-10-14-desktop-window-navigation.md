# 桌面窗口与导航验证（2026-10-14）

## 行为

桌面创建窗口时设定居中与 760×560 逻辑像素最小尺寸；主屏工作区按 scale factor 换算，默认和最小尺寸均限制在可用区域内，宽/高分别预留 32/48 逻辑像素。Android 自动创建窗口的路径不应用桌面限制。
宽度小于 1024 时顶部导航按钮打开共用菜单对话框，选中页面后关闭；常驻侧栏可滚动。桌面侧栏直达页面没有返回箭头，其余子页面返回显式目标或路由父级，直接打开子页面也可返回。

## 验证与边界

- 项目 nix develop 下应用双目标 build/lint/format、client-core 和 UI verify 通过。
- cargo build --workspace、verify:rust 通过，含桌面壳 host clippy 和 Android target check。
- agent-browser mock：760×560 下可展开完整菜单，选中配置后关闭；Escape 关闭菜单并将焦点恢复到按钮，无横向溢出。
- mock：直接打开 /config/route/rules 后返回 /config/route；1100×720 下常驻侧栏可见、代理页无返回箭头。
- 浏览器仅证明布局和路由行为。尚未在 Windows 真机验证原生缩放下限、窗口居中、多屏和高 DPI 工作区行为。
