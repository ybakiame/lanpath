# ADR-0012：GPUI 桌面壳本地验证

- 状态：Proposed
- 日期：2026-10-18
- 决策来源：初始用户要求在独立分支记录架构决策并开始本地验证，且不得影响现有 Tauri 分支迭代；初始范围未授权发布、全量迁移或替换现行桌面壳。用户随后明确授权本验证分支在本机使用原生 Windows Rust/MSVC 工具链。
- 后续范围：用户已批准 [ADR-0013](0013-gpui-experimental-parallel-migration.md) 的实验性完整业务迁移开发，不含生产替换或发布。本文件保留原验证阶段的入口、命令与结果，不作为提升后的应用启动指南。
- 环境入口已由 [ADR-0015](0015-standalone-devenv.md) 迁移至 standalone devenv；下文 `nix develop` 命令与结果保留为原验证阶段记录，不应重用为现行入口或新迁移的验证证明。当前操作指引见 [开发指南](../development.md)。

## 背景

ADR-0001 确定的现行架构是桌面与 Android 共用 Tauri/React 单壳双目标，桌面继续使用 sing-box，Android 使用 Kotlin VpnPlugin 与 Go libbox。
Windows 运行采样表明，桌面 UI 的 WebView2 多进程结构是工作集、线程和句柄的重要组成部分，但 CPU 占用较低；私有内存与 GPU 收益仍需同场景原型测量，不能由进程数或工作集直接推断。

GPUI Kit 已提供桌面窗口、组件、编辑器和测试能力，但移动端完整应用、现有 NSIS/updater 产物、Windows 双架构和原生权限行为不能由框架支持矩阵自动推定。
因此先进行可中止的本地验证，不把技术调研写成已接受的发布架构。

## 决定

验证范围限定为 Windows 桌面壳与 UI。保留 Android 的 Tauri/React、Kotlin 和 Go libbox 链路，不借本次验证扩大 macOS/Linux 发布承诺，不重写 sing-box，不引入第二个运行核心。

原型采用单一 `gpui-kit` 依赖及其默认上游 GPUI 后端，初验不同时引入 `gpui-fast`、直接 GPUI 依赖或嵌入式 WebView。
现有 `lanpath-client`、`lanpath-engine`、`lanpath-common`、`lanpath-mitm` 和 `lanpath-script` 是复用边界；如验证需要提取框架中立状态或命令逻辑，必须保留 Android 的 Tauri 薄适配，不复制业务语义。

本验证分支可在本机 Windows 使用现有原生 Rust/MSVC/CMake 工具链构建和运行原型。该例外仅限 ADR-0012 的隔离验证，不改变其他分支及 CI 的 Nix/原生分工；缺失组件或需要安装、升级、替换时必须另行请求用户处理，不自动修改宿主配置。

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

## Connections 切片验证记录

独立验证程序在 `apps/gpui-validation` 内提供编辑器、代理策略、连接、统计与运行日志五个页面，共用隔离核心工具栏。
`ConnectionsView` 独立拥有连接快照、轮询与请求任务，复用 `lanpath-client::connections` 的真实 API，不初始化 `ClientState`、系统代理或 TUN。

- 可见连接页正常每 2 秒发起读取；请求单飞，关闭操作与读取互斥，关闭后立即重新读取；读取失败保留最后成功快照并退避 10 秒。离开连接页暂停页面刷新，已在途请求可以完成；统计切片新增的 tracker 采集不随切页暂停。
- 选择按连接 ID 保存；快照变化后通过窗口观察回调同步 `ListState` 的选中索引，避免已关闭连接的索引高亮或键盘选择被错误继承给下一条连接。
- 核心启停使旧请求代数失效并清空快照；旧会话结果不能回写新会话。关闭成功与后续刷新失败分别展示。
- 交互采用“选中行 + 关闭所选连接”，不是现行 Tauri 页的逐行关闭按钮；当前会话关闭缓冲和持久统计见下文。过滤、排序与详情仍未迁移，不宣称 Connections 全量等价。

当时的检查命令（历史记录，非现行入口）：

```sh
nix develop --no-pure-eval --command cargo fmt --manifest-path apps/gpui-validation/Cargo.toml -- --check
nix develop --no-pure-eval --command cargo clippy --locked --manifest-path apps/gpui-validation/Cargo.toml -- -D warnings
nix develop --no-pure-eval --command cargo build --locked --manifest-path apps/gpui-validation/Cargo.toml
```

Windows 在获准的原生 MSVC 环境运行 `cargo build --release --locked --manifest-path apps/gpui-validation/Cargo.toml`。
运行前将 `LANPATH_GPUI_CORE_BINARY` 指向隔离复制的 sing-box，将 `LANPATH_GPUI_CORE_DATA_DIR` 指向独立临时数据目录；代理/API 分别使用 `17899/19090`，不使用现有实例端口。
真实连接场景为 Windows 本地慢速 HTTP 服务与通过隔离 HTTP 代理访问它的 curl；以核心 `/connections`、curl 断开结果、原生窗口画面及 UI Automation 选择状态交叉核对，不使用 mock 连接。

Stats 已接入真实 tracker 与统计存储，Logs 已接入应用 tracing 与真实核心订阅，见下文。不得用活动连接计数冒充统计历史或用虚构文本补充日志。

本次结果：上述 WSL 格式、clippy 与 build 通过，X11 真实窗口检查了导航和未配置状态；Windows x64 release 构建通过，保留共享 `lanpath-common` 中既有的未使用导入警告，未修改共享 crate。
原生场景确认自动出现真实连接、流量字节增长、选择关闭后核心列表归零且 curl 报连接重置、新连接不继承旧选择、核心故障时保留快照并独立显示关闭错误、停核清空列表。
首轮窗口带核心直接退出时，隔离进程与监听消失，但仍遗留 PID 文件。统计切片已新增先停 tracker、停核心、再关窗的异步序列，并验证 PID 文件删除，见下文；强制结束进程不在该保证内。
验证前后现有 Lanpath app/core PID、`7899/7900/9090` 监听及系统代理均未变化。

Windows x64 release、连接页可见、一个真实慢速 HTTP 连接，连续 10 秒采样：

| 进程 | 工作集 MiB | 私有内存 MiB | 线程 | 句柄 | CPU（单核百分比） |
| --- | ---: | ---: | ---: | ---: | ---: |
| GPUI 验证 UI | 62.7 | 77.9 | 28 | 1214 | 1.405 |
| 隔离 sing-box | 36.2 | 58.3 | 18 | 274 | 0.156 |

这不是完整客户端的同负载对照，不能据此承诺最终内存或 CPU 收益。此次未新增 Windows ARM64、高 DPI/IME、UAC、TUN、安装升级或 Android 验证；GPUI/Windows ListItem 外层 UIA 名称仍为空，选择验证使用 `SelectionItemPattern`，不宣称读屏覆盖完整。

## Tracker 与持久统计切片验证记录

`CoreValidationService` 唯一持有 `ObservationService`：在隔离目录准备 `StatsStore`，启动真实核心后创建带 SQLite sink 的 `ConnectionTrackerHandle`。
停止时先取消日志订阅并等待任务退出，再消费 tracker handle 并等待其取消，然后停止核心；存储实例保留给离线查询，下一次启动创建新 tracker，禁止用 drop handle 或第二个页面 tracker 代替显式停止。
连接页仍使用 HTTP 验证核心可达性，避免把 tracker 无错误回执的旧快照误报为实时数据；“会话已关闭”显示当前 tracker 的最多 500 条内存缓冲，停核或新会话清空。

统计页独立拥有请求、分段选择和展示快照，SQLite 查询全部在 GPUI 后台执行：

- 今日概览直接读取 `today_summary`，含总上下行、已代理上下行、连接数和去重目标数；直连、拦截不计入已代理。
- “今日日聚合”读取 `query_daily`，按复合维度、流量降序显示前 200 条，不将截断结果冒充目标/出站排行榜。
- “持久关闭明细”读取 `query_records`，跨日期按检测关闭时间倒序显示前 200 条；SQLite 行 ID 与 Clash 连接 ID 不混用。
- 运行且页面可见时每 5 秒刷新；停核后仍可查询。即使未设置核心二进制，已有隔离目录的统计也可打开；首次读取失败不显示虚构的零汇总。
- 日期采用共享 `stats::today()` 的本地日期口径；日聚合保留 90 天、明细保留 7 天，只在打开存储时清理过期记录。临时目录可以被系统清理，不承诺永久保留。

持久化保证仅限共享实现已成功提交的 SQLite 事务。tracker 的 `stop()` 是任务取消，不是最终快照、drain、flush 或 WAL checkpoint。
快照间消失的短连接、最后采样后的尾流量，以及取消时尚未提交的批次均可能漏记；跟踪器的采样/写入错误只有 tracing 记录，没有公开健康回执或丢批重试。本轮不修改共享 crate 来宣称更强保证。

窗口关闭通过 `on_window_should_close` 延迟：禁用新核心/代理操作，等待已接受命令完成，在后台显式停止日志订阅、tracker 与核心，成功后移除窗口；失败则保留窗口并展示错误。
不依赖 GPUI 仅 200ms 的应用退出回调窗口完成该序列，也不在窗口线程执行数据库或核心等待。

本轮验证：

- WSL：独立应用格式化、clippy `-D warnings`、locked build 通过；真实 X11 窗口以不可用目录触发统计错误，确认未显示零流量替代错误。
- Windows x64：原生 locked release 构建通过；仍只有共享 `lanpath-common` 的既有未使用导入警告。
- 真实本地 HTTP 长连接经隔离核心转发；停留编辑器页时，SQLite 已记录 1 个连接、1 个目标及 96360 B 下载，已代理流量为 0，证明采集不依赖统计页挂载。
- 通过连接页关闭后，会话关闭缓冲与数据库明细均出现真实记录；停核后统计页仍可读取。
- 同一应用停止并重新启动核心后产生第二个连接，累计连接数恰为 2、同目标聚合行连接数为 2；最终已提交上传 158 B、下载 928976 B，持久关闭明细 2 条，没有因双 tracker 重复计数。
- 正常带核心关窗后，进程、监听及 PID 文件均回收；重新打开应用且不配置核心二进制时，界面仍显示上述汇总与两条明细。使用统一 UTF-8 的原生检视命令比对退出前后完整查询结果相同；最初 CP936/UTF-8 混用造成的箭头文本比对差异已确认属于验证脚本解码问题。
- 点击启动后 6ms 内请求关闭窗口，仍正常退出，隔离核心、监听和 PID 文件均未残留。

未更改 Tauri、Android、共享 tracker 或统计存储契约；未新增 ARM64、高 DPI/IME、UAC、TUN、安装更新验证。Stats 的筛选、日期选择和清空流程尚未迁移；Logs 的磁盘日志与导出尚未迁移。上述数据库检视命令与 HTTP 服务均为本地临时验证工具，不进入源码或 CI。

## Runtime Logs 切片验证记录

2026-10-19，本地验证程序新增真实运行日志，不依赖 Tauri 日志模块或生产实例日志目录：

- 应用来源：进程启动时安装 tracing layer，仅捕获 `lanpath_` 前缀目标，保留事件级别、目标及结构化字段；不把 UI 提示文本伪装为核心输出。
- 核心来源：订阅隔离 Clash API 的 `ws://127.0.0.1:19090/logs?level=trace`，按真实 JSON `type`、`payload` 解析。协议依据为 [sing-box 的 `getLogs` 实现](https://github.com/SagerNet/sing-box/blob/testing/experimental/clashapi/server.go)，并以本机真实 sing-box 1.14.2 验证；不声称转发全部 stdout/stderr。普通输出配置为 INFO，本次 API 订阅实际也收到 DEBUG，展示以真实帧级别为准。
- 时间使用本地接收时刻的 RFC3339，按进程内入队序号倒序展示，不冒充核心原始事件时刻。订阅前及断流期间不回放，不保证完整启动日志或无丢失。

`CoreValidationService` 唯一拥有日志任务。核心启动后开启订阅；停止或正常关窗时，先取消并等待该任务，再停止 tracker 与核心。订阅只持有缓冲，不反向持有 service。
连接超时为 5 秒，断开后每 2 秒重连并显式展示连接中/断开状态；HTTP 4xx 或 204 显示不可用，重新启动核心后重试，不以空列表伪装健康。

缓冲与展示契约：

- 环缓冲最多 1000 条，满时淘汰最旧条目并累计计数；清空只影响本次内存及淘汰计数，不停止采集、不删除文件，序号不复用。
- 单条消息最多 8 KiB，按 UTF-8 字符边界截断并标记；tracing 字段格式化也有界，截断后不继续拼接后续字段。WebSocket 单帧与单消息上限均为 64 KiB。
- 视图最多另持 1000 条 `Arc` 快照；暂停刷新可能保留已被环缓冲淘汰的旧快照，直到下一次刷新。8 KiB 是消息内容上限，不是整行或进程内存上限。
- 提供全部级别、DEBUG+/INFO+/WARN+/ERROR 最低级别过滤，默认 INFO+；显示最新 200/500/1000 条，默认 500。页面可见且自动刷新开启时每 2 秒同步；暂停只冻结展示，离页不停止采集。切页进入、改变筛选、手动刷新、清空和核心操作完成时可以主动同步。
- 列表使用固定单行预览，避免虚拟行高与长消息换行冲突；选中后在只读编辑器查看保留的完整文本。按日志序号维持选择；所选日志不再处于当前结果集时清除详情，不将旧选择套到新条目。

本轮已执行：

- WSL：`nix develop --no-pure-eval` 内执行独立应用格式化、`cargo clippy --manifest-path apps/gpui-validation/Cargo.toml --locked --all-targets -- -D warnings`、`cargo test --manifest-path apps/gpui-validation/Cargo.toml --locked --lib`、`cargo build --manifest-path apps/gpui-validation/Cargo.toml --locked`，全部通过。5 个回归用例覆盖容量/过滤/清空身份、UTF-8 内容边界、跨字段截断、真实帧格式校验及选择随插入/淘汰的转移；跨字段边界先复现失败，再修正通过。
- 仓库回归：同一 Nix 环境内 `cargo build --workspace`、`bun run verify:rust` 全部通过，覆盖根 fmt/clippy/test、现行客户端壳 host clippy 和 `aarch64-linux-android` Rust check；保留既有 ignored 用例，未执行 Kotlin/AAR 构建或 Android 实机验证。
- Windows x64：原生 MSVC `cargo build --release --manifest-path apps/gpui-validation/Cargo.toml --locked` 通过，仍保留共享 `lanpath-common` 的既有未使用导入警告。真实窗口同时显示应用启动/核心生命周期 tracing 和核心代理请求日志。
- 3 次真实 HTTP 代理请求指向本地未监听目标；暂停期间视图保持 0 条，手动刷新得到缓冲 15 条，INFO+ 显示 12 条，ERROR 显示 3 条；没有通过暂停停止后台订阅。
- 停留编辑器页发送 400 次真实请求，返回日志页后缓冲及显示均为 1000 条、已淘汰 1015 条；切为 200 条只改变展示，不改变缓冲。截图发现并修正了首轮长行重叠，最终真实窗口的千条列表与 ERROR 过滤均无该重叠。
- 结束本轮隔离核心后，日志页保留历史并显示断开及原生 socket 错误；显式停核后显示已停止，重新启动后恢复连接，新请求产生新日志。
- 通过原生 UIA 读取选中详情，确认完整保留目标与错误尾部；新请求插入后详情文本保持不变，清空后移除。另发送 250 次真实请求达到 1000 条缓冲、淘汰 255 条，所选旧事件被淘汰后详情移除。
- 原生点击详情后键入，以及 UIA `SetValue` 均未修改详情文本；但 UIA `IsReadOnly` 仍报告 `false`、焦点暴露为 Window，不能宣称无障碍语义完整。既有 ListItem 外层名称为空的限制也未解决；未读取或修改系统剪贴板来验证复制。
- 带实时订阅正常关窗后，验证 UI、隔离核心、监听及 PID 文件均回收。原生产 app/core PID、`7899/7900/9090` 监听与系统代理保持原值。

本轮仍为 Proposed 的本地功能切片，不构成完整 Logs 文件管理、完整桌面迁移、发布就绪或性能收益结论；不改变 Android、现行 Tauri、系统代理、TUN、更新或安装契约。原生场景脚本、截图和数据仅用于本地验证，不进入源码或 CI。
