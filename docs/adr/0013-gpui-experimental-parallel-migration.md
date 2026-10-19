# ADR-0013：Windows GPUI 实验性并行迁移

- 状态：Accepted（仅实验性并行开发范围）
- 日期：2026-10-19
- 决策来源：用户明确选择“进入实验性并行迁移”，授权开始完整业务接入；保留 Tauri 稳定实现和 Android，隔离身份与数据，不包含生产切换、系统代理/TUN 接管或发布授权。

## 背景

[ADR-0012](0012-gpui-desktop-local-validation.md) 已验证原生 Windows 窗口、编辑器、共享 Rust 服务复用、真实核心生命周期、代理与连接、持久统计和运行日志。证据足以开始完整业务迁移开发，不足以宣布现行桌面替换、性能收益或发布就绪。
验证程序的固定核心配置和样例编辑器不是订阅、覆写和设置业务的替代实现。下一阶段按完整用户任务接入真实数据与共享服务，不以增加演示页面作为迁移进度。

## 决定

### 两个应用，一个业务实现

- `apps/client` 继续承载现行 Tauri 桌面及 Android；现有安装、更新、签名和便携身份不变。
- GPUI 实验应用入口为 `apps/desktop`，使用独立 Cargo 项目和带实验标识的可执行程序/窗口。它不嵌入 WebView，不加入 Android 目标，不自动替换现行桌面入口。
- 复用 `lanpath-client`、`lanpath-engine`、`lanpath-common`、`lanpath-script` 和 `lanpath-mitm` 中适用的业务能力；Tauri 与 GPUI 分别适配窗口和平台事件。共享语义不在两个壳内各自实现。
- GPUI 的草稿、选择和任务状态由对应 Rust feature/entity 拥有，不把 React 的 Query/Jotai/Zustand 机制移植成第二个权威状态源；Tauri/Android 的现有状态规范保持不变。
- 保持 sing-box 单核心技术路线；实验实例和现行实例的进程、控制端口及数据根必须隔离。并行开发不等于允许共享写入或共同控制全局网络状态。

### 数据和系统权限

- 实验应用不自动读取、迁移或删除 `~/.lanpath`、现行 Portable 数据、WebView 数据或生产数据库。只使用自身数据或用户明确提供的测试输入。
- 实验数据根必须能在重开应用后恢复其自身已保存内容；“临时目录可被系统清理”不能充当常规持久化契约。
- 核心监听与 Clash API 限于实验实例的 loopback 端口；本阶段不启用 TUN、不调用系统代理设置，不允许配置编辑绕过该边界。
- 后续系统代理/TUN 验证必须单独确定控制权与恢复方案并取得授权，不能因本 ADR 已接受而默认放行。
- 不新增发布密钥、安装器/updater 身份、自动迁移、强制 CI 门禁或 Android 发布步骤。ADR-0012 的本机原生 Windows Rust/MSVC 验证例外继续适用于本实验目标；其他环境仍按仓库 Nix 规范。

### 迁移单位和切换条件

每个迁移单位完成真实输入、持久化、业务操作、错误恢复和原生行为验证；已经暴露的操作不得是占位、假成功或样例数据回显。未迁移能力在覆盖记录中明确，不以同名页面宣称对等。

正式替换仍需独立决策，至少应有完整任务覆盖、数据兼容和回退、系统集成、所需目标架构、安装更新路径、复杂输入/无障碍与同负载资源对照的实际证据。Tauri 在该决策前保留可用，不以本 ADR 接受状态推定已经取代 [ADR-0001](0001-independent-client.md)。

## 后果

当前批准的是可撤回的实验性完整业务迁移开发，不是两个正式发行版，也不是生产数据双写。验证成果保留在 ADR-0012；本 ADR 仅扩大开发阶段，不抹去已有的验证限制或把缺失能力改写为已完成。

## 迁移覆盖基线

基线为批准本阶段时的 `0e87b89`，按完整用户任务统计，而不是按能打开的页面统计。以下缺口不因应用改名而视为完成。

| 用户任务 | 既有复用边界 | 阶段开始时 GPUI 覆盖 |
| --- | --- | --- |
| 添加/编辑订阅、刷新节点与用量、选用、查看/导出原文 | `lanpath-client::subscription`、`SubscriptionStore` | 未接入 |
| 创建覆写、校验 YAML/JS/远程 URL、保存、绑定订阅并重启生效 | `ProfileStoreV2`、`validation`、`profile::overrides` | 仅编辑器技术验证，无业务闭环 |
| 预览最终配置、启动所选订阅、启动阶段与失败反馈、重开使用已保存值 | `profile::build_core_config_v2`、`core_config`、`state/start`、engine | 仅固定配置的隔离启停和正常退出 |
| 全部策略组、节点选择持久化、测速、规则/全局/直连模式 | `proxies`、`persist_group_selection`、`set_rule_mode_persist` | 仅固定 `proxy` 组的即时选择 |
| 活动连接、会话关闭记录、流量与路由、关闭连接 | `connections`、`ConnectionTrackerHandle` | 已接真实服务，尚非完整业务会话集成 |
| 今日/历史统计、日期/关键字/排序、清空、离线重开 | `StatsStore` | 已有真实汇总/默认查询，缺完整查询及清空交互 |
| 运行日志、暂停/过滤/全文、磁盘日志尾读及导出 | 应用 tracing、核心日志 API、现行 Tauri 日志适配 | 已有真实内存流，磁盘/导出未接入 |
| DNS/出站/策略/默认路由/缓存的结构化编辑、验证和持久化 | `ConfigSlicesStore`、`ConfigSlices::validate`、`dns_probe` | 未接入，固定 JSON 字段不算编辑能力 |
| 规则/规则集 CRUD、市场、分类、更新、引用、缺失资源降级与补齐 | `LocalOverrideStore`、`RuleSetManager`、`ruleset_update` | 未接入 |
| 远程脚本/片段、导入、参数、刷新、调度与任务结果 | `RemoteManager`、`import`、scheduler、`lanpath-script` | 未接入，样例 JS 不等于执行/管理 |
| MITM 白名单、CA、信任和抓包记录 | `state/mitm`、`lanpath-mitm`、平台证书适配 | 未接入；CA 系统变更另行授权 |
| 请求代理、GitHub 镜像、外观及重开恢复 | `ClientConfig`、`request_network`、现行主题偏好 | 未接入，不以 GPUI 默认主题冒充迁移 |
| age 备份、解密预览、确认恢复、停止、重启事务与外观恢复 | `lanpath-client::backup`、平台文件适配 | 未接入，不能以普通 JSON 保存代替 |
| 核心版本管理、端口/IPv6/面板、系统代理/TUN 授权与回滚 | `cores`、`privilege`、`sysproxy`、engine | 仅显式指定独立核心；全局系统控制未授权 |
| 安装/Portable、种子/许可资产、通知/opener、单实例、更新和回退 | 现行桌面 storage、平台适配、updater、发布工作流 | 未接入，现行身份与发布契约不变 |

接入时保留这些业务约束：

- 唯一生效订阅为 `active_subscription_id`，模板通过订阅的 `profile_id` 关联；订阅贡献节点，不直接接管本地策略和规则。
- v2 合成顺序保留节点、本地模板、slices、远程 YAML、本地 YAML、远程 JS、本地 JS，随后再 compose、local override、面板特性及规则集物化。JS 的 `main` 文本检查不是语法/执行或核心配置校验。
- 预览可能拉取/缓存资源，不作为严格只读且必然等同运行配置的 API；保存成功不等于已热生效。
- 请求策略保留 none/system/core/custom，镜像只改 URL；原文查看/导出读取成功缓存，不重新请求或冒充合成配置。
- 备份保留白名单、age、一次性预览和启动事务及 v1/v2 兼容；规则集保存保留 backing files 和后端更新元数据。
- 不以初始化 `ClientState` 绕过实验边界：当前 `ClientState::stop()` 会调用系统代理关闭，即使启动开关为 false。实验壳按能力接入或提取框架中立编排，不能用无操作平台适配假装已支持。

## 首个业务迁移单位：实验配置文档

原验证程序已干净迁至 `apps/desktop`，删除样例 YAML/JS、只读演示与固定运行 JSON。
新实体 `ConfigView` 拥有真实 UTF-8 JSON 草稿、脏状态和单飞操作；`ConfigStore` 负责原文加载、
能力边界、真实核心 checker 和同目录原子保存，`DesktopRuntime` 负责已保存配置的重新校验与生命周期。
没有把 `ProfileStoreV2` 的 YAML/JS 覆写存储冒充完整核心文档，也未调用会重新合成字段的面板 builder。
订阅、覆写合成、结构化 slices 等仍按上表缺口接入；此单位不是这些任务的替代实现。

- 默认持久根与两个新环境变量、启动/检查命令见[开发指南](../development.md#windows-gpui-实验性并行)。
- `DataRoot` 在服务创建前验证祖先、所有权与文件树，持有真实 OS 独占锁至服务和 executor 释放；
  拒绝目录不会进入配置或离线统计服务，不以伪平台适配绕过权限。
- `configuration.json` 是唯一保存源；首次新建文档未保存不可启动。校验不保存，保存不热应用，
  启动只消费重新加载且通过同一 checker 的已保存值。失败保留原文件和当前草稿。
- 运行/切换/配置操作期间只读；重新载入与正常关闭处理未保存确认，关闭等待已接受任务再停日志、
  tracker 和核心。已有实时代理、连接、统计、日志继续使用同一隔离实例。
- checker 是选定核心的 `check -c … -D …` 子进程，不是文本模拟；20 秒超时并回收，持续排空输出，
  有界保留诊断。能力策略不取代核心 schema，也不会静默改写不被允许的字段。
- 本阶段固定 loopback mixed/API；拒绝重复 JSON 键、Go 大小写字段别名、额外监听、TUN/系统副作用、
  隐式外部文件与未审定能力；资源仅根内显式相对路径，日志/缓存输出限各自目录。
- 路径边界同时拒绝核心的 `$` 环境变量展开和 Windows 硬链接；`..` 在 Windows 绝对化可能消去之前检查。
  这是受控实验目录的准入和配置约束，不是对同用户恶意运行期篡改的 OS 沙箱。

## 本轮验收（2026-10-19）

| 范围 | 实际执行与结果 |
| --- | --- |
| 独立应用 / Nix | `fmt --check`、`clippy --locked --all-targets -- -D warnings`、`test --locked --lib`、`build --locked` 通过；38 passed、2 ignored |
| 现行 Rust 回归 / Nix | `cargo build --workspace --locked`、`bun run verify:rust` 通过，含根 fmt/clippy/test、Tauri 壳 host clippy、`aarch64-linux-android` Rust check |
| Windows x64 / MSVC | release build 与 lib tests 通过；35 passed、2 ignored；保留 `lanpath-common` 既有未使用导入及链接器信息警告 |
| Windows / 真实核心服务 | 以隔离复制的 sing-box 1.14.2 显式执行 `real_core_checks_exact_saved_source_and_rechecks_before_start`，1 passed |

普通套件的 ignored 为真实核心显式选择用例和跨进程锁测试入口；后者由父用例实际启动子进程执行。
未修改前，新增 hosts 环境变量路径回归在 Nix 失败，Windows 硬链接回归失败；修正后两平台通过。
原有 `..` 拒绝用例又在 Windows 暴露绝对化先行的问题，修正校验顺序后通过。

Windows 真实窗口使用键盘输入、UI Automation、私有 API、真实 loopback HTTP 和截图交叉验证：

- 首次不生成保存文件、不可启动；Unicode 节点名与首尾空格按原文保存。Ctrl+Enter 只校验，
  不改正文、不保存、不启动；Ctrl+S 保存、Ctrl+R 触发重新载入确认。
- 以保存的“持久直连A/B”启动，Clash selector 实际返回对应成员，HTTP 请求真实通过实验 mixed 端口。
  运行中尝试键盘修改未改变正文；停止后鼠标/键盘可继续编辑。
- 无效 JSON、仅真实核心能拒绝的 `server_port` 类型错误和 TUN 配置均保留草稿及上一份已保存原文。
  诊断在独立只读区域显示，实际大窗口和 900×700 窗口无本轮所见的编辑区/工具栏重叠。
- 重新载入、关闭的取消都保留草稿，显式丢弃才继续；重开恢复最后保存值且不自动启动。
  运行中正常关闭回收私有核心、日志/tracker 和 PID 文件。
- 离线破坏已保存 JSON 后，重开显示原损坏文本；启动明确失败，无固定配置回退，不覆写原文；
  编辑修复、保存、启动可恢复。
- 同根第二实例被锁拒绝，源编辑器未读取主实例原文；切到统计仍显示不可访问。
  自建含 `client.json` 的拒绝夹具未增加 marker/lock/日志/数据库等文件，原内容不变。
- 不设置数据根覆盖变量时，实际在默认 `%LOCALAPPDATA%\LanpathExperimental` 保存、关闭、重开和显式启动，
  正常关窗清理核心/PID，证明常规持久根不依赖临时目录。
- 生产 Tauri/核心 PID 保持 9096/20816，7899/7900/9090 仍由原核心监听；
  ProxyEnable 保持 1、ProxyServer 保持 `127.0.0.1:7899`。实验 17899/19090 无残留监听，
  私有复制核心无残留进程。

本轮截图中的非法换行来自应用校验快捷键被输入组件优先消费，并非应当放宽 JSON 解析。
绑定改为匹配 `Configuration > Input` 后实测正文不变且校验成功；校验完成提示也不再被 dirty 状态遮挡。
未保存确认在真实窗口存在；工具初次按不可访问的标题查找而误超时，随后以实际按钮和截图验证取消/丢弃。
UIA 的 disabled/焦点/readonly 等部分元数据仍与真实行为不一致，不能据此宣布完整无障碍支持。

这不是性能对照、ARM64 发行、系统代理/TUN 接管、Kotlin/AAR/APK 构建或 Android 真机验收。
本轮未执行这些项目，不据 Rust target check 宣称 Android 运行行为正确；完整业务替换仍按切换条件另行决策。
