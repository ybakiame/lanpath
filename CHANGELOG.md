# Changelog

## [Unreleased]

- 桌面窗口默认居中、最小尺寸 760×560 逻辑像素；小屏和高 DPI 下按工作区限制启动及最小尺寸。
- 窄窗口新增可展开主导航，侧栏可滚动；桌面非侧栏直达子页面提供返回上一级入口。
- 修复切换订阅后启动仍显示旧代理节点：切换完成前禁用启动，启停/重启时取消旧请求并重置策略和连接缓存；策略读取失败提供实际错误与重试入口。

## [0.1.1-beta.1] - 2026-10-11

### Added

- GitHub Actions 统一使用 `.yaml`；Windows Build 与 Android Build 分别复用测试和发布构建。

- 首次公开测试版：main 分支 CI、Windows x64/arm64 NSIS 与 Portable、Android arm64 release 签名 APK、独立 updater 发布密钥、验签与 SHA256 清单。

- Windows x64/arm64 新增 Portable ZIP，使用可执行文件旁的 `data` 保存配置、核心、证书、日志和 WebView 偏好；更新引导至发布页，保留数据手动替换程序。

- 主题设置集中到二级页，移动端统一 Lanpath 外观；深色外观新增可持久化的 OLED 纯黑背景开关，跟随系统进入深色时也生效。

- 客户端页面、共享控件、状态提示与加载指示统一使用 lucide-react，移除 Heroicons 依赖及内联路径。

- 移动端采用大圆角卡片、连接主视觉、卡内启停操作和胶囊底部导航，配置入口使用自适应网格。
- 外观设置新增鸢尾紫、湖水青、玫瑰粉与品牌蓝强调色，支持浅深主题独立配色、持久化与首帧预置。
- 岚径 Lanpath 独立客户端工程，正式身份 com.ybakiame.lanpath。
- 自有 Rust/React/Go/Kotlin 模块、Nix、协议子模块及客户端验证和发布流程。

### Fixed

- Windows/Linux 安装版 WebView 数据归入产品目录的 `webview` 子目录，避免使用 identifier 目录；便携版移动后重定位托管核心和证书路径，不自动复制、删除或迁移旧用户目录。

- 桌面共享二级页移除历史返回箭头，侧栏导航继续保留页面标题与操作；移动端仍显示返回按钮。

- 保留源仓未提交的 Windows MITM 控制台抑制和双端节点选中标签移除修复。
- 保留 Android IME 避让、订阅裁剪、导航高亮及双端 UI 整理成果。
