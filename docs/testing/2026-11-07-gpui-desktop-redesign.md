# GPUI 桌面重设计验收

范围为独立 `apps/desktop` 的窗口、主题、侧栏、配置导航和页面呈现。设计见
[桌面设计说明](../desktop-design.md)。本记录汇总本批交付证据，不保存逐次操作日志。

## 构建与检查

从仓库根进入 devenv，独立桌面执行：

```sh
devenv shell -- cargo fmt --manifest-path apps/desktop/Cargo.toml --check
devenv shell -- cargo clippy --manifest-path apps/desktop/Cargo.toml --locked --all-targets -- -D warnings
devenv shell -- cargo test --manifest-path apps/desktop/Cargo.toml --locked --lib
devenv shell -- cargo build --manifest-path apps/desktop/Cargo.toml --locked
```

Windows 镜像使用既有锁定 Rust 1.98.0/MSVC/SDK/CMake 原生工具链执行同组独立项目检查，
未安装或升级宿主组件。检查按阶段集中执行，局部修复只重跑受影响项：

| 阶段 | 已执行与结果 |
| --- | --- |
| 主题与页面重设计 | 两端 fmt、严格 all-target Clippy、build 通过；Linux 库测试 157 通过 / 5 忽略，Windows 154 通过 / 11 忽略 |
| 外观最终定向检查 | 两端 fmt、严格 Clippy、build 通过；`--lib appearance::` 在 Linux 13 项、Windows 12 项通过 |
| 核心配置导航调整 | 两端 fmt、严格 all-target Clippy、build 通过；Linux 库测试 158 通过 / 5 忽略，Windows 155 通过 / 11 忽略 |
| 滚动视口与紧凑标题修正 | Linux 严格 Clippy 与 build、Windows 原生 build 通过，定向复查配置与最小窗口 |
| 最后侧栏与窗口文案调整 | 两端格式化及 build 通过；按用户要求未重跑测试、Clippy 或 GUI 调试 |

外观检查覆盖旧配置缺少 material 字段、材质往返、无效值拒绝、损坏文件和保存失败保护、
备份保留本机缩放与材质、系统外观解析，以及强调色和中性文字对比度。
以上分阶段证据不表示最后一次完整源码状态重新执行过全部行为验收。

## Windows 原生场景

使用新编译的 Debug GUI、脚本自有子进程及新建临时实验数据根。UI Automation 按名称或 ID
重新读取，必要时按已解析的可见边界操作；截图来自真实 HWND。产品脚本为
[native_ui_review.ps1](../../apps/desktop/tools/native_ui_review.ps1)，需 PowerShell 7。

脚本显式接受 `Binary`、`WindowTools`、`OutputDirectory` 参数。`WindowTools` 指向本机
工作区上层的 `test-tools/windows/DesktopWindow.psm1`，提供窗口发现、滚动、点击、截图与
尺寸操作；该外部辅助模块不随本仓库分发，使用脚本前须由调用者提供，不是产品依赖或 CI 门禁。
原始截图、无障碍树、日志、临时目录与本机路径不入库。

| 场景 | 已观察结果与范围 |
| --- | --- |
| 首轮完整 UI | 60 项断言、32 张截图，stderr 为空；当时的全部 19 个目标与页面内容对应，明暗、强调色、纯黑及材质切换落盘 |
| 键盘导航 | F6 / End / Enter 跨折叠组进入关于，Home / Enter 返回订阅，Ctrl+, 打开外观 |
| 订阅菜单 | 回环 HTTP 夹具获取测试节点；更多菜单停用、启用落盘，删除对话框 Esc 取消后保留订阅 |
| 配置导航与草稿 | 后续 17 个可见目标与内容对应；配置定向 30 项断言、13 张截图通过，网络端口草稿跨分区保留，Ctrl+S 保存当前网络分区，规则三分区可到达 |
| 实际核心选择 | 复制已有真实版本二进制到临时库存，经核心页选择 Mihomo 后只显示预览占位；键盘跳过 sing-box 专属入口，切回 sing-box 保留分区选择；仅浏览库存类型不改变菜单 |
| 最小窗口 | 960×640 / 150% 下连接、统计、结构化配置与诊断可到达；紧凑标题修正后 9 项定向布局断言、7 张截图通过，stderr 为空 |
| 缩放与退出 | 恢复 100% 同步保存值与控件，正常关闭退出码 0；另复用既有窗口冒烟覆盖标题栏拖动、双击缩放、最小化和还原 |

`LayoutOnly` 只复查尺寸、缩放与关闭；`ConfigurationOnly` 定向复查配置。
可选 `MihomoFixture` 与 `SingBoxFixture` 指向已有真实版本目录中的二进制，两者同时提供时
才验收实际持久选择；版本探测与选择不等于启动核心。最后新增的关于快捷图标、核心版本
脚注及 slogan 仅完成编译，未由上述较早窗口记录验收。

## 发现与修复

- Sidebar 虚拟列表在焦点自动滚动时重试预绘制，同帧重复登记 a11y 节点，导致 Debug 下
  F6 / End 跨组展开崩溃。固定目标导航改用普通滚动容器和标准 SidebarMenuItem，保留
  域标识与焦点句柄；同一键盘路径通过，未关闭断言或无障碍来绕过。
- 导航和网络表单的无障碍分组须标记实际滚动视口，不能使用超出视口的内容边界；
  否则自动化会把底部固定工具或保存区误当作可滚动内容。
- 修正缩放选择器与恢复按钮溢出、状态区域空白和高缩放下摘要挤占列表空间；
  结构化配置在窄窗口使用紧凑标题，规则常态提示缩短，错误和脏草稿提示仍完整显示。

## 验证边界

本批未改协议子模块、共享业务、Tauri、Android、发行身份或网络接管准入。
未启动真实核心、改写系统代理、启用 TUN、安装 CA、迁移生产数据、发布或覆盖已安装程序。
订阅只使用回环夹具，无私有订阅或生产数据；未为验收修改宿主主题。
ARM64、其他桌面平台、Android、所有 DPI 与全部窗口尺寸未据此验收；显式忽略项仍未执行。

按用户决定，本轮只整理配置 UI：sing-box 切片、本地规则与 .srs 仍不应用于 Mihomo，
系统网络参数保持已有共享持久化与运行时行为；Mihomo 独立配置存储与校验留待后续开发。
本轮不声明完整 sing-box schema 编辑能力。
