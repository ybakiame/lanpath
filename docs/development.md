# 开发与工作流

Agent 可按任务读取仓库内的 [环境准备 skill](../.agents/skills/lanpath-environment/SKILL.md) 与
[构建验证 skill](../.agents/skills/lanpath-build-verify/SKILL.md)。入口见 AGENTS.md，工具未自动发现时可直接读取。
Skills 只组织既有流程；命令以 nix/commands.nix、包脚本和构建脚本为准，项目约束仍以 AGENTS 与本文为准。

先 git submodule update --init --recursive，再从仓库根进入 `devenv shell`；根目录运行 lanpath-install。非交互命令使用 `devenv shell -- COMMAND ...`。
协议子模块为私有治理资源，维护者需有访问权限；普通公开源码构建不依赖该子模块，可跳过其初始化。
本仓使用独立的 standalone devenv 配置，避免使用宿主工具链；Windows CI 例外见 ADR，GPUI 本机实验例外见下节。
主 nixpkgs 保持 nixos-26.05 的 `a033e66ab94a6bedf2c3d58d93a36319d3cbbbaf` 快照，Android SDK 输入保持 `b3c4566d2d946a76082ce9be34640d24f7eec477`；均由 devenv.yaml 声明并由唯一项目环境锁 devenv.lock 固定，迁移不升级输入。Bun 1.4.2 通过官方归档与 SHA256 固定，Rust 仍由 rust-toolchain.toml 固定。输入升级须单独授权：先明确修改 devenv.yaml 中的目标 revision，再用 `devenv update nixpkgs` 等定向更新锁文件；不以无参数 `devenv update` 顺带更新全部输入。随后执行受影响目标的完整验证，见 [环境指南](../devenv.md)。
CLI 固定为 devenv 2.4.0：`nix profile add --accept-flake-config github:cachix/devenv/b904dcb51fe48c30db250038241507f60752f222#devenv`。配置入口为 devenv.nix，工具、桌面、Android 和共用命令位于 nix/；可用 direnv allow 启用已提交的 .envrc。进入 shell 仅初始化环境，不安装项目依赖、不构建 AAR、不启动代理。入口与锁定机制取代关系见 [ADR-0015](adr/0015-standalone-devenv.md)。
本地及 CI 共用 lanpath-install、lanpath-rust-targets、lanpath-verify-frontend、lanpath-verify-rust、lanpath-test-tools、lanpath-build-aar 和 lanpath-build-android-debug/release；这些命令可从仓库子目录调用，失败立即退出。仍可在 shell 中直接使用原 Bun/Cargo 命令。发布签名只由既有 release 工作流显式注入，Husky 继续作为唯一提交检查入口。
初次准备使用 cache.nixos.org 与 devenv 官方签名缓存；devenv CLI/辅助程序自身的上游锁定构建快照不替换项目工具链，缓存未命中时仍可能从源码构建。standalone CLI 管理输入锁定和 shell 求值缓存；实际缓存命中与耗时受配置、依赖及本地状态影响，不承诺具体加速，也不将 shell 准备成功当作应用构建证据。
前端运行 bun run --filter lanpath-app dev 或 dev:android。
浏览器使用 http://localhost:1420/?browser-debug=1；mock 不能证明真实 IPC/设备行为。
移动端预览使用 Android mode 的 http://localhost:1430/?browser-debug=1。连接卡片水面仅为装饰，
不表示流量或质量；触摸仅产生一次涟漪，不捕获手势或改变启停行为。尊重系统减少动态效果，
离屏/后台暂停水面动画；本版未请求陀螺仪权限或接入传感器，真机耗电与滚动体验需设备验收。
共享 UI 的 CSS 扫描由 apps/client/src/index.css 的 @source 覆盖。
桌面壳默认居中创建窗口，常规最小尺寸为 760×560 逻辑像素；启动与最小尺寸按主屏工作区和 DPI 收敛，保留边框空间。宽度低于 1024 时顶部提供主导航按钮，菜单支持键盘关闭与焦点恢复；侧栏直达页面保留标题，其他子页面返回显式 backTo 或路由父级。
代理启停和重启成功后共用 writeRuntimeStatus 取消旧状态轮询/运行查询，重置策略与活动连接缓存；启动后刷新已挂载查询，停止时不重拉核心数据。缓存回归用 `bun test packages/client-core/scripts/runtime-queries.test.ts`（在本仓 Nix 环境）执行，CI 同步执行；浏览器 mock 不等于真实核心验收。

## Windows GPUI 实验性并行

窗口锁定/退出状态的传播必须幂等：相同状态不得再次刷新或通知观察它的父视图。
统计页曾因重复 `closing=false` 形成观察者闭环，导致 Windows 窗口高 CPU 且无法响应；
原生回归覆盖私有数据根启动、窗口消息响应及正常关闭，不启动核心或改动系统网络。

并行期间，GPUI 使用独立配置目录，Tauri 沿用现有标准目录；两者不共享可写根、不自动双写，
当前不搬迁或覆盖 Tauri 数据。正式切换时再将 GPUI 交接到 Tauri 的标准配置目录，
通过停机、迁移预览、事务应用与可恢复回退完成；并行版本不提前切换默认目录或放宽旧根保护。

入口为独立 Cargo 项目 `apps/desktop`，程序名 `lanpath-desktop-experimental`、窗口名
“Lanpath 实验版（GPUI）”。这是 [ADR-0013](adr/0013-gpui-experimental-parallel-migration.md)
批准的并行开发目标，不替换现行应用，也不改变安装器、updater、签名或 Android 入口。
旧 `apps/gpui-validation` 及 `LANPATH_GPUI_*` 变量已移除；ADR-0012 中旧命令仅为历史验证记录。

从仓库根在 Nix 环境分别执行独立项目检查；根 `verify:rust` 不覆盖此项目：

```sh
devenv shell -- cargo fmt --manifest-path apps/desktop/Cargo.toml --check
devenv shell -- cargo clippy --manifest-path apps/desktop/Cargo.toml --locked --all-targets -- -D warnings
devenv shell -- cargo test --manifest-path apps/desktop/Cargo.toml --locked --lib
devenv shell -- cargo build --manifest-path apps/desktop/Cargo.toml --locked
```

本机 Windows 原生验证使用已授权的 Rust 1.98.0/MSVC/Windows SDK/CMake，不授权安装或升级组件。
在具备这些工具的 x64 开发终端、Windows 本地源码副本中运行。正常启动不需要核心环境变量；
在“核心”页检查通道、下载并选中实验目录内的 sing-box，再生成预览和启动：

```powershell
Remove-Item Env:LANPATH_DESKTOP_CORE_BINARY -ErrorAction SilentlyContinue
cargo run --release --manifest-path apps/desktop/Cargo.toml --locked
```

`LANPATH_DESKTOP_CORE_BINARY` 仅保留为显式测试覆盖：可指向单独复制的真实核心，但不得指向现行
应用正在使用的核心。覆盖只作用于检查器和运行时，不改写已保存的库存选择；界面会提示覆盖，
并锁定下载、选择和删除。Windows 程序使用 GUI 子系统，不随应用另开控制台。

`LANPATH_DESKTOP_DATA_DIR` 可显式指定实验数据根；未设置时 Windows 使用
`%LOCALAPPDATA%\LanpathExperimental`，非 Windows 使用
`$XDG_DATA_HOME/lanpath-experimental` 或 `$HOME/.local/share/lanpath-experimental`，不回退临时目录。
首次只接受空目录或新目录，后续要求有效实验标记和独占 OS 文件锁；拒绝生产/Portable 祖先、
未知非空目录、符号链接/junction 和硬链接文件。不要手动伪造标记或删除锁文件；同时运行的第二实例
不会读取该根的配置或离线统计。这不是抵抗同用户恶意运行期文件替换的 OS 沙箱。

实验版以独立 `client.json`、订阅清单/成功缓存和覆写模板作为持久业务源，不再编辑完整
`configuration.json`。订阅页支持添加/编辑、刷新、启停、设为当前、绑定覆写模板以及查看/导出
最近一次成功原文；刷新失败记录错误并保留旧缓存。覆写页在一次保存前统一校验名称、本地
YAML/JS 和远程 URL；仍有关联订阅时拒绝删除。切换脏表单、删除和关闭未保存表单均需确认。

“预览”按共享 v2 顺序合成当前订阅节点、本地模板、结构化 slices、远程/本地 YAML、远程/本地
JS、local override、面板特性和规则集物化结果。预览只读，补齐实际引用的缺失规则集并调用选定
核心的真实 `check`；启动不复用预览文本，而是重新生成、再次准备资源并执行同一能力边界和核心
校验。订阅、覆写、结构化配置或规则保存会使预览重新生成；并发变更排队一次后重跑，避免旧预览
重新开放启动按钮。Ctrl+R 可手动重新生成。保存成功不热应用，运行和配置操作期间订阅、覆写、
结构化配置、规则及预览锁定；任一业务草稿未保存时拒绝启动，避免误把已保存旧值当作当前表单生效。

“代理策略”展示运行核心返回的全部组和成员；Selector 成员确认后先切换核心，再写入实验
`client.json`，下次启动等待 Clash API 就绪后逐组恢复有效选择，失效成员不阻止核心启动。可对所选
成员或当前整组测速；超时显示为无延迟结果，读取/操作失败保留最后成功快照并允许重试。

规则/全局/直连模式可在停核时保存，生成配置的 `default_mode` 使用该持久值；运行中切换还会
热推送 `/configs`。保存成功但热推送失败时明确保留“已保存、当前同步失败”状态，下次启动再次同步。
这些操作只影响实验数据根和 `19090` 控制面，不授权系统代理、TUN 或生产核心控制。

“结构化配置”直接编辑共享 `ConfigSlices`：DNS 跟随系统/接管、服务器、规则和探测，自定义节点、
Selector/URLTest 分组、内置组可调项、默认路由/域名解析器及缓存文件。数值输入保留无效原文并
阻止保存或切换选中项；删除服务器、规则和自定义出站需确认。一次保存先执行跨切片引用与协议
字段校验，再原子替换 `config_slices.json`；失败保留草稿和上一份文件。DNS 与出站引用使用实际
sing-box tag，内置分组名称、协议和动态成员仍由共享模板规格维护。DNS 探测只请求当前服务器，
不修改持久配置；结果或错误显示在同页。

“规则”直接编辑共享 `LocalOverride`：路由规则支持全部 match/action/advanced 字段、顺序和
多规则集引用，第一个启用的 Final 规则覆盖结构化默认路由。规则集支持远程 source/binary、手写
source、分类批量整理、单项/全部智能更新和显式恢复内置项；删除规则集后失效引用会保留并提示，
不静默改写规则。市场从现行 MetaCubeX 目录源生成 geoip/geosite 条目，可选择 GitHub Raw 或
jsDelivr。一次保存原子替换完整 local override，但规则集更新只合并后端检查状态，不覆盖并发分类
编辑或复活已删除项；预览和启动准备实际引用的缺失 backing files。

“脚本”管理实验根中的远程纯 JS 与 Surge/Loon 片段，支持清单 CRUD/启停、URL 嗅探、头部元数据、
模块参数、立即更新、配置导入以及 cron 任务状态和手动运行。更新间隔会持久化，但当前没有自动刷新
编排，资源只在用户点击“立即更新全部”时拉取。远程资源/导入草稿会阻止核心启动；运行中锁定清单
和导入写操作，但允许查看及手动运行已启动调度器中的任务。核心停止和正常关窗会等待调度器退出。
“MITM”复用脚本缓存中的 rewrite、hook 与派生 Hostname，支持开关、手动白名单、隔离 CA、按当前
CA 指纹查询系统信任、显式确认安装、PEM 复制和进程内抓包记录。手动 IP 白名单按精确 `/32` 或 `/128`
路由，域名继续按完整域名或 `*.` 后缀匹配。启用且存在有效白名单时，核心启动动态增加
`main-in`、`lanpath-mitm` 与固定 `127.0.0.1:17900` 回流入站；空白名单保持普通单入站。
CA/私钥只写实验根，页面不会自动安装 CA、设置系统代理或启用 TUN。验证时不得为了通过测试直接
触发“安装到系统信任库”；系统证书变更仍需用户单独授权。

左侧按“配置 / 运行 / 扩展”分组导航。“连接”使用同一后台 tracker 的活动/关闭快照和字节总量，
按连接 ID 保留选择，详情提供 IP、出站链、规则、字节和开始时间，可关闭所选活动连接。
“统计”可在停核或重开后查询：日聚合支持本地日期、搜索、精确出站和全部排序字段，关闭明细
支持搜索与全部排序字段；均可设置升降序和 1–1000 条返回上限。明细 API 没有日期边界或精确
出站条件，不以 UI 假过滤冒充后端能力。统计行保持单行预览，选中后可在只读详情中查看和复制
完整字段；选择按聚合复合键或明细主键保持。清空确认作用于全部实验聚合/明细，不限当前筛选；
清空会等待已接受查询完成并使旧快照失效，运行中的跟踪器仍会采集新数据。

“日志”区分内存与磁盘：应用按天写入 `logs/app.log.YYYY-MM-DD`，核心实时订阅写入 `logs/core.log`，
读取尾部最多 1000 行 / 2 MiB，可选择复制；不会回放订阅前或断流期间的核心日志。
导出按文件名顺序拼接真实日志原文至实验根 `exports/`，不含启动配置，也不包含旧导出；
这是未加密文件，可能包含访问目标，分享前需检查。清空内存缓冲需确认，不删除磁盘日志或导出。
正常关窗会等待统计清空、日志读取/导出等已接受操作，日志后台写线程随 runtime 关闭排空队列。

“网络请求”复用共享 `ClientConfig` 与请求管线，支持不使用代理、系统、实验核心和自定义
HTTP(S)/SOCKS5；只有实验首装默认 `none`，后续读取保留已保存策略、认证信息与 GitHub 镜像前缀。
停止核心且其他配置操作结束后才可保存，保存只合并请求字段，不覆盖订阅选择、模式或 MITM 设置；
修改用于后续请求，不改系统代理。核心代理不可用时报告失败，不回退直连或系统代理。
GitHub 镜像沿用共享域名白名单和前缀拼接规则；内置规则集镜像回退也读取该已保存前缀。
连接测试调用共享 GitHub 探测，以接受操作时的已保存快照请求 `api.github.com/zen`，不保存或使用
草稿；每次请求超时 10 秒，沿用资源拉取重试规则。自定义地址在 UI 整体遮蔽，但凭据以明文保存于
实验 `client.json`，不是加密凭据库。重载及关闭脏草稿需确认，正常关窗等待已接受的测试完成。

“外观”将 `theme`、`accent`、`pure_black` 和 `scale_percent` 原子保存到独立 `appearance.json`，
不写核心配置或读取现行 WebView 存储。主题支持浅色、深色及跟随系统；强调色为品牌蓝、鸢尾紫、
湖水青、玫瑰粉，默认跟随系统和品牌蓝。纯黑仅在解析为深色时生效，切换浅色保留选择。
缩放为 75–150 的整数百分比，通过主题基础字号和 rem 同时缩放控件、文字与间距，不只放大文字。
启动首窗前应用保存偏好，跟随系统由保留的窗口外观订阅更新。选择先保存成功再应用；读写失败
明确显示错误，损坏文件不自动覆盖，修复后可显式“重新载入外观”。

“核心”与现行 Tauri 共用 `CoreManagementService`，展示本地版本和 stable/beta/prerelease 的
实际远端结果。下载复用已保存的请求策略与 GitHub 镜像；解压至隔离 staging，版本探测精确匹配后
才发布并原子合并 `core_binary`。失败不改变原选择或覆盖既有核心；旧未完成版本目录可补齐，
但不删除其中其他文件。staging 不作为已安装库存，实验分支不下载尚未启用的 TUN 专用 DLL。
选择仅接受自身 `cores/sing-box/<version>/<平台文件名>` 下无链接且可执行的库存；删除需确认，
且不能删除当前选中项。运行或启停、其他配置操作、诊断及退出期间锁定核心修改。
下载只有连接和元数据请求超时，没有归档传输总超时；正常退出等待已接受的核心操作结束。

每次启动重新读取持久核心选择，不复用窗口创建时的管理器。界面通过 watch 通道显示实际经过的
读取选择、运行数据、端口预检、MITM、合成与资源、核心校验、脚本任务、进程、API、选择恢复、
统计与日志阶段；保留本次会话最近一次启动的完成/失败/警告与实际耗时，不估算百分比。
失败后回收已启动服务和核心；重试建立新记录，不保留上一次失败作为当前阶段。

“诊断”以接受运行时的已保存实验配置和实际进程状态调用共享分步探测，支持 Enter 和按钮提交，
显示目标、序号、各步状态/耗时/摘要/详情与失败总数；调用失败保留旧报告并明确其目标。
缺失资源结合启动物化报告与当前运行配置中本地 backing files 的实时检查；文件缺失不等于核心
已丢失内存中的规则，不声称自动修复或全部路由已降级。诊断会实际执行系统 DNS、直连 TCP、
已配置 DNS、实验 mixed/API 路径，不等同于全局请求代理测试，也不能证明 TUN 或其他应用流量正常。
表单和报告由同一页面滚动区承载，放大和缩小窗口后仍可到达；正常关窗等待已接受诊断完成。

“关于”显示真实构建版本、OS/架构、租用的数据目录与当前持久库存选择，可显式重载；
不在渲染时执行核心、不提供安装更新或系统设置操作。选中信息是库存快照，不是运行状态证明。

“备份”与现行 Tauri 共用 `lanpath-client::backup::BackupService`，沿用 age 加密、v1/v2 格式、
16 MiB 密文 / 64 MiB 展开 / 1024 文件限制及五分钟一次性预览。导出密码可生成、显隐和显式复制；
原生保存对话框仅保存密文，目标必须在实验数据目录之外，已有文件不会覆盖。选择 `.zip.age`
文件时检查普通文件、大小与链接；选择后保留已读取的密文快照，重新选择才读取文件的新内容。
取消文件对话框不改变来源或配置；修改导入密码/来源、检查失败或预览过期会撤销恢复授权。

解密只显示版本、平台、时间、数量、大小和外观摘要；无效归档的错误不携带解密字段值。
“准备恢复…”先明确确认覆盖及丢弃脏草稿，再等待已接受操作、消费有效令牌、停止核心与写入者，
并保存使用同一导入密码加密的恢复前副本。准备成功后原配置仍未替换，编辑和启停锁定，须显式
“立即重启并恢复”；最新回滚副本同样经过密码、预览和确认流程，不能直接覆盖。

重启替换进程通过私有继承管道等待旧进程实际退出，不以固定延迟猜测目录锁释放。新进程先取得
实验目录独占租约、处理启动事务，再创建业务和统计存储；普通第二实例仍直接拒绝，不等待或抢锁。
恢复保留本机所选库存核心，系统代理/TUN/MITM 不会自动开启。启动结果及失败需要显式确认；
成功重试清理旧失败标记，中断与拒绝事务保留既有恢复语义。

外观只迁移共享协议中的主题、强调色、纯黑模式，本机缩放不变；不支持的强调色保留本机值并
在预览和恢复后说明。损坏的本机外观文件不自动覆盖，保存失败保留恢复摘要和明确警告。
正常关闭等待已接受的文件选择、加密/解密或准备操作完成，不把后台工作留给已退出的窗口。

生成配置仍固定 mixed 为 `127.0.0.1:17899`、Clash API 为 `127.0.0.1:19090`，MITM 生效时
另使用 `127.0.0.1:17900` 回流端口；占用则失败，
不接管占用进程。拒绝额外监听、TUN、系统代理/时钟改写及未审定能力；资源必须落在实验根内，
不接受环境变量展开、系统 hosts 默认路径或外部 UI/自动证书资源。文件日志和缓存输出分别限于
根下 `logs/`、`cache/`。能力边界先检查，具体配置 schema 仍由同一真实核心负责。

可用 `LANPATH_TEST_SING_BOX` 显式选择真实核心，单独执行普通套件跳过的生成配置与生命周期回归：

```powershell
$env:LANPATH_TEST_SING_BOX = Join-Path $env:LOCALAPPDATA 'LanpathExperimentalCore\sing-box.exe'
cargo test --release --manifest-path apps/desktop/Cargo.toml --locked --lib business::tests::generated_subscription_pipeline_starts_real_core -- --ignored --exact
cargo test --release --manifest-path apps/desktop/Cargo.toml --locked --lib business::scripts_tests::fetched_remote_task_runs_while_real_core_is_live -- --ignored --exact
cargo test --release --manifest-path apps/desktop/Cargo.toml --locked --lib business::mitm_tests::generated_pipeline_records_real_mitm_traffic -- --ignored --exact --nocapture
```

Linux 主机构建不代表 Windows/ARM64 发布或 Android 设备验收；本阶段没有新增 CI 门禁。

第 12 单位目前仅有 [ADR-0013 中的系统集成设计草案](adr/0013-gpui-experimental-parallel-migration.md)，
不表示功能已迁移或权限架构已采纳。实施顺序为设置/绑定、系统代理会话、经单独决策的 TUN、
证书与真实 HTTPS 验收；当前固定端口、能力拒绝和不自动安装证书的行为不变。
系统代理应用与归还、UAC/TUN/路由/DNS、临时 CA 安装及移除分别需要明确许可，
并在无生产实例的专用 Windows 环境验证；发现生产代理或身份冲突应拒绝接管，
不能自行停止生产程序。源码调查和只读状态核验不能替代这些系统效果验收。

## Android

ADB 已连接设备的选择、按授权安装、日志采集和原生行为验收见 [Android 调试 skill](../.agents/skills/lanpath-android-debug/SKILL.md)。

在根目录执行 bash apps/client/scripts/build-lanpath-core.sh，生成未跟踪的 lanpathcore.aar。
进入 apps/client 执行 bun run tauri android dev --target aarch64，使用前端热更新。
编译 CLI 会重建被忽略的 tauri.settings.gradle 与 RustWebView JNI 胶水；不要提交本机生成路径。
不要跨检出共享 Android CARGO_TARGET_DIR；Tauri Kotlin 生成文件可能被旧构建缓存漏掉。
正式 APK 使用 bun run android:build。调试已有草稿的设备前先征得安装/重启许可。

## 工作流

`.github` 中的 YAML 文件统一使用 `.yaml` 扩展名。当前工作流为 `ci.yaml`、`windows-build.yaml`、`android-build.yaml` 与 `release.yaml`。

| 类型     | 使用时机     | 发现与计划                               | 实现与验证                                 | 文档与最终复查                       |
| -------- | ------------ | ---------------------------------------- | ------------------------------------------ | ------------------------------------ |
| feature  | 新用户能力   | 读取领域、平台边界与 ADR，列验收         | 最小完整实现，相关 verify/gates 和行为验收 | CHANGELOG、架构/API 文档，复查范围   |
| bugfix   | 既有行为错误 | 复现、定位根因，列回归路径               | 最小修复、真实可用的回归检查               | 记录根因与验证边界，复查无旁支变更   |
| refactor | 结构改变     | 确认基线、行为不变目标                   | 分步重构、相关 gates                       | 更新路径/架构，核对输出语义          |
| docs     | 说明变更     | 核对实现和命令证据                       | 修改并验证链接/命令                        | 消除冲突，复查权威与措辞             |
| adoption | 协议采纳     | 读 ADOPT，Pass 1 inventory/mapping/audit | 无决策冲突后 Pass 2 生成、协调、验证       | schema/conformance/项目检查与 report |

全部工作流按 AGENTS 完成定义验收；冲突、未经授权的架构/门禁/数据变更和未知验收标准
触发 AGENTS 停止条件。适用能力由项目事实决定，不生成无用途的 Agent/MCP/Prompt 工作流。

## 订阅原文与覆写验收

在订阅卡片点击「原文 / 导出」，核对 YAML 注释/换行、JSON 排版与 base64 文本原样保留，
导出扩展名按实际响应格式确定，内部节点缓存仍为 JSON。旧订阅刷新后才有原文，刷新失败保留
上次成功内容；修改 URL 后原文不可用，直到重新刷新。列表与编辑表单不再配置覆写。
首页「订阅覆写」为当前生效订阅选择模板；保存期间禁止启动/切换订阅，运行中提示重启。
模板管理链接在移动和桌面均可进入。实际 Downloads 落盘、Android MediaStore、覆盖安装与
代理启动后的节点更新须在目标设备验收，浏览器 mock 和主机检查不能替代。

Windows TUN 授权使用 SDK 的 Administrators SID 常量；UAC 成功后正常退出旧实例，
取消授权保留原实例，已有权限时不重复提权。原生 Windows SID 测试与真实 UAC/TUN 须在目标平台验收。

## 网络请求设置

设置 → 网络请求对订阅添加/刷新、预览/启动缺缓存、远程资源与核心下载统一选择
不使用、系统、核心、自定义代理；GitHub 访问只配置 URL 镜像前缀。自定义支持 HTTP(S)/SOCKS5，
保存对后续请求生效。系统模式无配置时直连，Linux 主要读取环境变量；代理不可用会返回错误。
旧开关升级先生成 client.request-proxy-v1.backup.json，再原子写入新配置，保留开启→核心、
关闭→直连的语义。恢复时退出客户端，以备份替换 client.json；保留备份直至确认新版本稳定。
回归与设备边界见 docs/testing/2026-10-16-subscription-and-windows-tun.md 和 ADR-0007。

## 备份与恢复

现行 Tauri 设置 → 备份与恢复，设置至少 12 字符密码或生成 24 位随机密码，单独保存密码后导出。
Tauri 桌面文件位于 Downloads/Lanpath；Android 经 MediaStore 写入公共下载目录。GPUI 实验版
通过原生对话框选择导出位置，操作边界见上面的实验版说明。导入 .zip.age 后输入密码查看摘要，
确认完整替换；代理停止后重启生效。Android 点击关闭应用按钮后，从桌面重新打开。
恢复前加密副本使用同一导入密码，可通过「使用恢复前备份」再次导入。核心、证书/私钥、
日志、流量历史、系统授权不迁移；跨设备的外部文件引用需检查，系统代理/TUN/MITM 手动重开。
新格式和中断恢复见 ADR-0008，测试记录见 testing/2026-10-17-encrypted-backup.md。

Android 启动先授权再准备配置，跨页 busy 由共享 mutation key 读取；备份预览五分钟有效、确认一次消费。
密码派生热循环在根与独立壳的 dev/release 均启用 opt-level=3，不降低加密参数，见 ADR-0009。

新导出备份为 v2，包含 rulesets 本地文件；导入兼容 v1/v2，旧备份缺文件时仍需联网。
启动阶段通过独立 proxy_startup_progress 查询展示，避免被运行状态锁阻塞；只补下载被启用路由/DNS 引用的缺失远程规则集。
见 ADR-0010 与 testing/2026-10-18-ruleset-backup-startup.md。

规则集分类独立于 GeoIP/GeoSite 内容类型，空值表示未分类；分类批量整理只改分类，不需重启核心。
规则集更新通过条件 GET 与文件内容判断，时间仅展示；更新检查由用户触发，不逐条调用 GitHub API。
旧 local_override.json 升级前保存 ruleset-metadata-v1 副本，回退需关闭客户端后替换。见 ADR-0011。
