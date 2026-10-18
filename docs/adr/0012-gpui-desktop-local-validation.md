# ADR-0012：GPUI 桌面壳本地验证

- 状态：Proposed
- 日期：2026-10-18
- 决策来源：用户要求在独立分支记录架构决策并开始本地验证，且不得影响现有 Tauri 分支迭代；本次未授权发布、全量迁移或替换现行桌面壳。

## 背景

ADR-0001 确定的现行架构是桌面与 Android 共用 Tauri/React 单壳双目标，桌面继续使用 sing-box，Android 使用 Kotlin VpnPlugin 与 Go libbox。
Windows 运行采样表明，桌面 UI 的 WebView2 多进程结构是工作集、线程和句柄的重要组成部分，但 CPU 占用较低；私有内存与 GPU 收益仍需同场景原型测量，不能由进程数或工作集直接推断。

GPUI Kit 已提供桌面窗口、组件、编辑器和测试能力，但移动端完整应用、现有 NSIS/updater 产物、Windows 双架构和原生权限行为不能由框架支持矩阵自动推定。
因此先进行可中止的本地验证，不把技术调研写成已接受的发布架构。

## 决定

验证范围限定为 Windows 桌面壳与 UI。保留 Android 的 Tauri/React、Kotlin 和 Go libbox 链路，不借本次验证扩大 macOS/Linux 发布承诺，不重写 sing-box，不引入第二个运行核心。

原型采用单一 `gpui-kit` 依赖及其默认上游 GPUI 后端，初验不同时引入 `gpui-fast`、直接 GPUI 依赖或嵌入式 WebView。
现有 `lanpath-client`、`lanpath-engine`、`lanpath-common`、`lanpath-mitm` 和 `lanpath-script` 是复用边界；如验证需要提取框架中立状态或命令逻辑，必须保留 Android 的 Tauri 薄适配，不复制业务语义。

应用身份、版本与更新签名身份、`~/.lanpath` 和 Portable 数据布局、NSIS/updater ZIP/Portable ZIP、种子核心与许可资产均保持不变。
本阶段不自动迁移或删除 WebView 数据，不改变外观偏好既有处理，不新增托盘、单实例、deep link 或新强制 CI 门禁。

验证使用独立分支、worktree、构建目录和临时数据根，不读取或修改正在运行实例的业务数据，不停止其进程，不占用其监听端口。
初验不改变系统代理、TUN 授权或安装状态；涉及这些行为的后续验证必须先有隔离方案和明确授权。

## 本地验证范围

1. 依赖与窗口：在原生 Windows MSVC 环境解析锁定依赖，创建真实 GPUI Kit 窗口，验证中文文本与输入、键盘焦点、窗口缩放和高 DPI；不得只以 WSL/Linux 编译结果代替。
2. 运行时边界：保留可用 Tokio runtime/reactor，系统调用与核心 I/O 不阻塞窗口线程；验证框架中立服务可被 GPUI 与现有 Tauri 适配共同调用。
3. 编辑器：验证 Override 所需的 YAML、JSON、JavaScript、只读、搜索、换行和中文输入行为，记录与 CodeMirror 的实际差异。
4. 资源：在相同运行时长和负载下分别记录 UI 与核心的进程数、工作集、私有内存、GPU、线程、句柄和空闲 CPU，不把核心连接开销计入 UI 收益。
5. 平台与发布：后续按需验证 UAC、OS 通知、opener、退出清理、Windows x64/arm64 构建、旧 Tauri 安装版到 GPUI 本地安装包的升级及 Portable 数据保留；本 ADR 不要求在本轮执行安装或发布。

每项验证记录实际命令、目标平台、结果和未覆盖边界。真实窗口与原生行为不能由 mock、无头测试或其他操作系统结果替代。

## 后果

本 ADR 不取代 ADR-0001，不修改现行架构、开发入口或发布契约；验证分支可以独立迭代或中止，主分支和另一 Tauri 工作流不受其未提交改动影响。
本地生成的依赖、构建产物、临时数据、安装包和测试配置不得提交；不得为验证创建或替换发布密钥。

只有在真实窗口、资源、核心生命周期、数据兼容和所需发布路径均有证据后，才可请求将本 ADR 或后续 ADR 转为 Accepted。
届时需明确对 ADR-0001 单壳双目标条款的部分取代关系，并同步 AGENTS、架构、开发、运维、验证记录和 CHANGELOG；未获该决策前，现有 Tauri 桌面壳仍是唯一现行实现。
