# 开发与工作流

Agent 可按任务读取仓库内的 [环境准备 skill](../.agents/skills/lanpath-environment/SKILL.md) 与
[构建验证 skill](../.agents/skills/lanpath-build-verify/SKILL.md)。入口见 AGENTS.md，工具未自动发现时可直接读取。
Skills 只组织既有流程；命令以 nix/commands.nix、包脚本和构建脚本为准，项目约束仍以 AGENTS 与本文为准。

从仓库根进入 `devenv shell`，再运行 lanpath-install。非交互命令使用 `devenv shell -- COMMAND ...`。
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

验证按相对完整的功能批次集中执行与影响范围对应的全量检查；局部修复先执行定向检查，
收尾补齐受影响门禁，不因无关文档或排版变化重复运行已通过且代码未受影响的全量构建。

## Windows GPUI 实验性并行

并行期间，GPUI 使用独立配置目录，Tauri 沿用现有标准目录；两者不共享可写根、不自动双写，
当前不搬迁或覆盖 Tauri 数据。正式替换与旧目录交接另行执行停机、迁移预览、事务导入与
可恢复回退；默认便携不自动切回旧根，也不放宽现有生产目录保护。

桌面视觉与交互规范见 [GPUI 桌面设计](desktop-design.md)。页面共用标题、工具栏、表面与说明
折叠组件，业务状态仍由各 feature entity 持有；全量功能冒烟不能替代布局与高缩放验收。

GPUI 窗口使用组件库自定义标题栏，保留拖动、双击缩放和平台窗口控制。Windows 11 默认优先通过
DWM 系统 Acrylic 背景启用亚克力，旧系统保留 `WindowBackgroundAppearance::Blurred` 回退；
背景只由 Root 绘制一次，标题栏和侧栏叠加轻度色调；内容表面、菜单和弹层保持不透明。
在“外观”可切换实色材质，立即清除系统背景并切换原生窗口背景能力；亚克力不透明度支持
20–100%，默认 85%，选择保存后立即生效。深色纯黑强制不透明，保留所选材质以供退出纯黑后恢复。
其他平台保持不透明背景。

首个 RC 的候选源码版本为 `0.2.0-rc.1`，验收范围限定为 Windows 非 TUN。用户要求先完成以下
功能与真实验收，再合并主线、本地生成签名便携包；不把源码升版或已有 IPv4 冒烟当作 RC 通过：

- 授权与生命周期：普通权限启动、停止、核心异常及 GUI 死亡后的自有资源回收；按实际终止操作
  报告 Forced/AlreadyStopped，不把退出码推断为 Graceful。系统代理由独立普通权限守护持有并
  完成变更、回读与持久 checkpoint，GUI 经身份绑定、有界 IPC 请求；恢复策略见 ADR-0014。
  守护退出时状态明确为需要恢复；用户已手动清回原始有效配置时，Windows 自动连接数据差异不再阻止恢复，
  其他软件造成的未识别有效配置变化不视为已恢复，也不授予覆盖权限。
  已完成接管、原为直连的会话在用户手动关闭后，即使停用地址/绕过项已改变，也可在确认
  有效/类型值均直连、PAC/策略未变、所属持久记录匹配且再次回读稳定后，仅释放记录与租约；
  保留用户设置，不写系统代理。基线为外来代理或接管未确认时不适用。
  正常运行中的自有代理租约与 MITM/辅助会话不标记“需要恢复”；明确代理冲突、或停核后
  仍持有代理与资源才进入恢复提示。启停回调和后台轮询共用判定，保留原有清理证明要求。
- 网络矩阵：显式 HTTP/SOCKS、受控直连/Shadowsocks 上游、IPv4/IPv6 目标、代理侧域名解析、
  TLS 校验、WinINet 应用与精确归还，以及失败不静默直连。用户态 DNS 用例不代表系统 DNS
  接管，loopback IPv6 不代表公网 IPv6 可达。系统效果场景保留基线、独立本地超时与恢复证据。
- 脚本兼容：冻结下文 Surge/QX 明文 HTTP/MITM 子集，用实际导入、缓存和执行分别证明；
  不以局部资源导入宣称整个 Rules 仓库、厂商解析器、面板或管理 API 可用。
- 发行恢复：真实既有本地签名 `0.1.0` 包到更高候选版本的成功升级和中断恢复，覆盖便携与
  独立标准目录；保留数据、未知文件及冲突证据。既有本地签名包不是已发布历史 Release，
  同版本/空目标拒绝不等于升级成功，进程中断恢复不代表断电持久性。

产品 `tun_supported=false` 时拒绝 TUN 启动，保留已保存值且不进入提权。TUN/UAC 暂存、
系统 DNS/路由接管、最终布局/缩放/无障碍/性能和真实数据迁移均后置；不因此退役 Tauri 或改写
Android。首 RC 以实际 Windows x64 证据为准；ARM64 交叉构建与 Linux/macOS 检查不能
替代对应平台原生运行，公开发布仍需另行授权。

本批 Windows x64 原生行为验收（既有普通用户会话，实际受控核心，不以 mock 代替）：

| 范围 | 已运行的行为与边界 |
| --- | --- |
| 生命周期 | 实际 GUI 启停、核心强制退出、GUI 强制退出、同数据根重开与核心重启；系统代理的正常停止、核心崩溃、GUI 死亡均通过生产守护恢复，精确比较五项类型值、缺失状态、有效 WinINet 与受保护策略值，验证 journal 删除及守护退出；没有强杀守护或直接写注册表回滚。 |
| 网络 | 显式 HTTP/SOCKS × 直连/真实 Shadowsocks × IPv4/IPv6 × HTTP/HTTPS 共 16 条 literal 正向路径；另有四条代理侧域名解析与两条本地解析路径。22 条响应均比较精确 nonce 和目标侧实时 TCP peer PID。空信任 TLS、NXDOMAIN、DNS 丢弃、SS 停止不静默直连及 IPv6 关闭均验证负向结果；进程仍存活与未到达源站分别检查。 |
| 系统代理应用 | 使用真实 WinINet 默认代理客户端访问受控域名并证明经过自有核心；三种退出场景均在独立 Windows 本地看护与超时下执行，恢复依据生产 owner 策略，而非清空 ProxyEnable 或还原不明外来值。 |
| 脚本 | 自编 Surge/QX 样例实际走共享导入、缓存激活和 QuickJS/MITM 执行，资源服务器关闭后 HTTP/HTTPS 均返回精确正文与方言响应头；空信任拒绝、拒绝上游接受数为零、原客户端配置字节不变。复用既有 CA，不重新生成或安装；HTTPS 客户端显式信任该证书。此项是共享执行层证明，不是本轮 GUI/核心整链或 Windows 证书安装的重新验收。社区样例仍限于下文逐项记录。 |
| 升级恢复 | 既有本地认证签名 `0.1.0` → `0.2.0-rc.1`：生产替换路径在首个原子替换后子进程直接退出，真实新签名程序恢复旧包，再完成较高版本升级；便携与标准模式均保留数据、未知文件、目录身份与模式。公开 `bun verify.ts upgrade` 另走一次真实升级并比较不透明数据及未知文件字节。不是同版本重试，也不宣称断电保证。 |

原生运行还暴露并修复两项实际问题：DNS 切片移除 `local` 后模板 HTTP 客户端留下悬空
解析器引用；Windows 已退出内核对象不再提供映像路径时，错误拒绝本可证明的停止/恢复。
前者只在 YAML/JS 之前修复仍归模板所有的引用，后者仍拒绝创建身份不符或存活映像不可证明
的终止请求；保留失败前/通过后回归，不通过修改夹具绕过产品缺陷。

本地原生工具实际为 Rust 1.98、VC 14.51、CMake 4.3；不等于锁定 VC 14.44/CMake 3.31 的
远程 CI 已运行。以上证明不扩展到公网 IPv6、PAC/WPAD、旧 TUN 恢复、真实资料迁移、
其他平台原生行为、全部厂商脚本或公开发行；独立 Ed25519 产物认证也不等于 Windows
Authenticode 代码签名。

默认便携包也由验签 ZIP 实际解压后启动：不设置数据根/核心覆盖，工作目录故意不同于程序目录；
关于页显示同级 `data`，两次正常退出、重开保留同一数据根身份。本批最终根 workspace
build/fmt/strict Clippy、904 项单测、3 项集成及 1 项文档测试通过，另有 4 项显式 ignored；
Windows GPUI fmt/strict all-target Clippy、148 项默认单测与 release 构建通过，11 项默认
ignored，真实签名升级恢复用例另行执行通过。受影响 `lanpath-app` 与 client-core verify
通过；原有 Android 条件警告、React Compiler 跳过优化及大 chunk 提示保留，未用抑制替代检查。

全机只读前后快照中的适配器、地址、DNS、当前用户/机器信任根与产品防火墙集合一致；
长批次内外部 Tailscale 接口少了一条 peer host 路由，未改写或强行复原。该差异的来源未证明，
因此不宣称全机路由完全不变或已证明 Tailscale 共存；这与逐场景精确归还自有 WinINet 设置
是不同结论。保留原始快照及差异证据，不对不属于本次会话的路由执行删除/重建。

入口为独立 Cargo 项目 `apps/gpui`，分发程序为 `lanpath-gpui.exe`，显示名为
“Lanpath GPUI”。[ADR-0013](adr/0013-gpui-experimental-parallel-migration.md) 记录并行迁移，
[ADR-0014](adr/0014-gpui-native-integration-and-distribution.md) 授权窄权限 helper 和独立分发身份。
不替换稳定 Tauri/Android，也不授权公开发布。普通 GUI 拒绝继承高权限 token；
helper 子命令先于 GUI、业务存储与重启初始化分派。

从仓库根在 Nix 环境分别执行独立项目检查；根 `verify:rust` 不覆盖此项目：

```sh
devenv shell -- cargo fmt --manifest-path apps/gpui/Cargo.toml --check
devenv shell -- cargo clippy --manifest-path apps/gpui/Cargo.toml --locked --all-targets -- -D warnings
devenv shell -- cargo test --manifest-path apps/gpui/Cargo.toml --locked --lib
devenv shell -- cargo build --manifest-path apps/gpui/Cargo.toml --locked
```

本机 Windows 原生验证使用已授权的 Rust 1.98.0/MSVC/Windows SDK/CMake，不授权安装或升级组件。
调试宿主可优先使用已安装的 PowerShell 7，保留 Windows PowerShell 5.1 的模块兼容入口；
这不是产品运行依赖，不修改系统默认关联或执行策略。编排可从 WSL 发起，但最终行为验收须
区分 Windows 本地产物/数据、源码共享路径和测试夹具的边界；不能把更换 shell 当作原生 API 修复。
在具备这些工具的 x64 开发终端、Windows 本地源码副本中运行。正常启动不需要核心环境变量；
在“核心”页检查通道、下载并选中实验目录内的 sing-box，再生成预览和启动：

```powershell
Remove-Item Env:LANPATH_DESKTOP_CORE_BINARY -ErrorAction SilentlyContinue
cargo run --release --manifest-path apps/gpui/Cargo.toml --locked
```

`LANPATH_DESKTOP_CORE_BINARY` 仅保留为显式测试覆盖：可指向单独复制的真实核心，但不得指向现行
应用正在使用的核心。覆盖只作用于检查器和运行时，不改写已保存的库存选择；界面会提示覆盖，
并锁定下载、选择和删除。Windows 程序使用 GUI 子系统，不随应用另开控制台。

`LANPATH_DESKTOP_DATA_DIR` 可显式指定 GPUI 数据根；未设置时 Windows 使用
`%LOCALAPPDATA%\LanpathGPUI`，非 Windows 使用 `$XDG_DATA_HOME/LanpathGPUI` 或
`$HOME/.local/share/LanpathGPUI`，不回退临时目录。ZIP 默认携带程序旁的空文件
`lanpath-gpui.portable` 与空 `data/`；完整解压后直接使用 `<exe目录>/data/`，不依赖 cwd。
需要标准目录时，先正常退出，再仅移除这个空标记文件；重新启动使用上述独立标准目录。
切回便携时同样先退出再放回空标记。不移动、合并、覆盖或删除任一模式原有数据；
需要搬迁内容时显式导入或恢复备份。没有界面模式切换，也不使用 Tauri 的目录作为后备。
首次只接受空目录或新目录，后续要求有效标记与独占 OS 文件锁；拒绝生产祖先、
未知非空目录、符号链接/junction 和硬链接。`.gpui-root-id` 是本根持久 UUID，
不由备份或导入替换，物理移动 Portable 后保留。此准入本身不是同用户恶意篡改的 OS 沙箱。
按用户批准的“私有目录优先”策略，Windows 数据卷须支持持久 ACL；目录须归当前用户所有，
只允许该用户、SYSTEM 和 Administrators。新目录在创建时即设置私有 ACL，存量开放权限
不自动修复。exFAT/FAT32 或外来账户权限不受支持，须改用当前账户的私有目录并显式导入、
重新授权；不要删除保护标记、修改校验或整体提权 GUI 来绕过。

GPUI 的受控 Windows 核心独立于 Tauri/Android 的官方种子；在本仓 Nix 环境构建：

```sh
devenv shell -- bun apps/gpui/tools/managed_core.ts x86_64-pc-windows-msvc
# ARM64 使用 aarch64-pc-windows-msvc；交叉构建不等于 ARM64 原生验收。
```

源码与补丁清单位于 `apps/gpui/managed-core/source.json`，产物默认写入独立
`apps/gpui/target/<target>/managed-core/resources/seed/`。分发工具将应用 PE 的编译期摘要
与种子绑定，旧应用或补丁变化必须先重新编译应用，不靠版本输出为高权限执行建立信任。
打包须使用已授权的 Windows SDK `mt.exe`，WSL 下通过 `GPUI_MT` 指定；只规范化待签名副本，
不会修改原始构建文件。签名仍使用独立 GPUI 身份，不能以关闭验签作为调试替代。

Windows 受信核心种子继续用于首次发布和受限 TUN helper，但“核心”页不再提供手动安装按钮；
页面只管理真实上游库存。GPUI 支持 sing-box/Mihomo、稳定/预发布通道、本地版本切换和当前核心
构建 Environment/Tags。Windows 已验收 Clash/Shadowsocks 订阅到真实配置校验、用户态启动、
实际转发、正常停止及监听消失；高权限 TUN 的接管/恢复须单独验收，当前 helper 只接受 sing-box。

helper 的代码暂存仅复制签名清单中的固定叶文件，并始终持有源文件与祖先目录租约。
清单采用 `/` 分隔相对路径；传入 Shell 前在现有 UTF-16 缓冲区中转换为 Windows 分隔符，
不重新解析路径或释放身份绑定。每个 Shell 批次只调用一次 `PerformOperations`；
复制完成后验证所有传入文件的 ACL/大小/摘要，再用独立批次晋升，失败保留可验证的部分状态。
真实 Windows 已通过完整签名资源暂存、重新认证及句柄释放后的普通用户写入拒绝验证；
这不启动 helper 核心或 TUN，不替代 GUI 更新后的启动、接管与恢复验收。
全部批次现复用同一个 STA 与 Shell 对象，仍在每个目录创建后验证权限并取得租约，
在复制与晋升之间完整验证传入文件；删除批次的普通确认标志不继承到后续复制或晋升。
这不等于 Windows 保证只弹一次 UAC，实际次数须单独验收，不以
[`FOFX_REQUIREELEVATION`](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags)
作为授权次数承诺。首次缓存准备不启动核心、不改网络，应先完成或取消该阶段，再武装网络
验收的 60 秒看门狗；不能把用户尚在处理多次 UAC 的超时归类为 GUI 卡死。

UAC 拆分令牌的受限端与完整端不保证具有相同 `AuthenticationId`。helper 必须匹配
GUI 受限令牌的 `TokenLinkedToken` 所指向的完整令牌登录标识，且 SID、桌面会话和
Limited/Full 类型仍须一致；不能删掉登录绑定或接受其他管理员凭据。真实 Windows
只读回归已确认本机关联组合的登录标识不同且可正确验证，测试进程始终保持普通权限。
在同一普通权限 Windows 开发终端可显式执行，不会弹出提权请求或启动核心：

```text
cargo +1.98.0-x86_64-pc-windows-msvc test --locked --manifest-path apps/gpui/Cargo.toml --lib privileged::security::token_tests::actual_windows_linked_pair_is_accepted_without_elevating_the_test_process -- --ignored --exact --nocapture
```

Windows 防火墙的 `Rules.Add` 拒绝 Rust 规范化产生的 `\\?\C:\…` 应用路径，即使
`SetApplicationName` 接受该值。只为防火墙属性借用同一映像的普通 DOS 路径表示，
不改变受保护会话路径、文件摘要或目录/映像租约。原生普通权限回归在修复前得到
`E_INVALIDARG`，修复后进入预期的 `E_ACCESSDENIED`，不会安装规则。

恢复只计活动适配器和实际网络效果。Windows 的地址表与接口表均可能保留已移除 Wintun 的
phantom 行；仅当精确 GUID 对应设备的
[`CM_Locate_DevNodeW`](https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_locate_devnodew)
普通查询明确不存在、phantom 查询明确存在，且合并全局地址/路由查询后没有地址、路由或 DNS，
才把该行视为非活动元数据。活动设备、未知节点、查询失败、残留设置或缺少创建回执仍阻止恢复；
不通过手工删除设备、驱动或日志伪造恢复。签名 Windows GUI 已实测恢复原故障会话、
创建有原生收据的新 TUN、安装并回读应用绑定防火墙规则，以及经核心 socket 转发到纯
Windows 回环 HTTP 目标。GUI 停核后，网卡、路由、地址、DNS、CurrentUser Root、产品规则
和 WinINet 代理均与本轮基线一致，受保护日志由产品自行清除。
本轮保持自动路由、系统代理与 MITM 关闭，不代表默认路由接管或 MITM 已完成验收；
未单独采集本轮成功停止的 `StopKind`，不把 UI 停止成功额外宣称为优雅退出。

后续原生验收已覆盖 `auto_route=true` 的 IPv4 直连 TUN：受保护日志的目录 UUID、SID、
GUI 和适配器收据与当前实例匹配，Windows 为固定 HTTPS 目标选择该自有接口；
普通权限客户端不设置显式代理、沿用系统 TLS 校验，取得 HTTP 200，核心双向流量计数增长。
测试先保存配置和系统基线，再武装独立的 Windows 本地 60 秒看门狗，最后才允许启动核心；
到期先正常关窗，额外 10 秒仍未退出时仅可按本轮授权终止保留句柄绑定的原测试 GUI。
正常超时回退已实测约 60.7 秒完成，原配置字节恢复、受保护日志清除，七类系统基线一致；
不依赖网络发送停止命令，不承诺到期瞬间已完成所有恢复。强制兜底分支尚未做卡死故障注入。
本轮仍关闭系统代理与 MITM，也不证明 IPv6、DNS、Shadowsocks 上游或全部网络接管场景。
验收探针须保留 JSON 日期的类型和 UTC 偏移；PowerShell 7 可直接返回 `DateTime`，
不能再用按本地文化解析字符串的方式丢弃其偏移。



受控 TUN 的文件引用按配置上下文识别：根证书、TLS 客户端证书/密钥、ECH 配置、
DNS hosts 与本地规则集须先冻结为资源字节，再写入所属受保护会话。
HTTP headers 与 hosts predefined 的键是数据，不按 `command`、`private_key_path` 等名称提升为能力。
helper 在物化资源后仍独立运行与编辑器相同的完整能力策略，拒绝未知或大小写别名的全局副作用。

窗口锁定/退出状态的传播必须幂等：相同状态不得再次刷新或通知观察它的父视图。
统计页曾因重复 `closing=false` 形成观察者闭环，导致 Windows 窗口高 CPU 且无法响应；
原生回归覆盖私有数据根启动、窗口消息响应及正常关闭，不启动核心或改动系统网络。
在仓库 Nix 入口准备的已授权原生 MSVC/Rust 环境中，设置
`LANPATH_TEST_GPUI_BINARY` 为真实 Windows GUI 程序的绝对路径后，可显式运行：

```text
cargo +1.98.0-x86_64-pc-windows-msvc test --locked --manifest-path apps/gpui/Cargo.toml --test native_window_lifecycle -- --ignored
```

此项默认忽略，不是新增强制门禁。原生进程仅使用临时数据根，失败只清理自己持有的子进程；
构建通过或 headless 测试不能替代真实 Windows 窗口验收。

Windows 窗口先以隐藏状态构建，视图及关闭回调完成注册后再通过 GPUI 激活显示；
不能在平台原生构造阶段暴露可关闭 HWND，否则立即关闭可能绕过 GPUI 窗口回收并残留事件循环。
验收必须包含“窗口刚可见便关闭”，不能只覆盖启动稳定后的退出，也不能用延长超时或强制结束进程代替。
隐藏构建阶段原生视口可能尚无有效高度；导航侧栏在有效尺寸到达后才挂载，
避免虚拟列表把初始无效视口计算出的预渲染范围永久缓存成空白导航。


实验版以独立 `app.toml` 的 `[client]`、订阅清单/成功缓存和覆写模板作为持久业务源，不再编辑完整
`configuration.json`。应用外观位于同一文件的 `[appearance]`，远程资源清单位于 `[resources]`，订阅清单独立保存为 `subscriptions.toml`；其他业务文档保留既有格式。可重建下载、图标和刷新进度分类放入 `cache/`，本地导入持久保存在 `resources/imports/`。
检测旧 `client.json` 或 `appearance.json` 时，先进入不可操作的迁移页，后台完成备份、转换和校验，
再创建业务页面。版本化迁移位于 `apps/gpui/src/migration/`，每次在 `backup/migrations/` 保存
旧文件、转换结果和清单，全部成功后移除原位置旧文件；失败不发布新配置或删除原件。已迁移目录
若遗留旧文件，会补做备份和清理且不覆盖有效 `app.toml`。详见 [ADR-0016](adr/0016-gpui-unified-app-settings.md) 与 [ADR-0018](adr/0018-gpui-resource-storage.md)。

订阅页采用卡片列表，卡片展示来源、节点数、格式、到期和流量用量；新增/编辑使用居中弹窗，
编辑、刷新、启停、设为当前和删除等操作位于选中卡片内，原文通过占窗口约 75% 高度的
只读弹窗查看/导出。新增/编辑表单使用居中弹窗，User-Agent 快捷项采用胶囊按钮。
刷新失败记录错误并保留旧缓存。
覆写页采用模板卡片，展示内容类型和远程来源；新增/编辑使用大型弹窗，YAML/JS 编辑器占窗口高度 75%，
原文预览使用只读大弹窗。保存前统一校验名称、本地
YAML/JS 和远程 URL；仍有关联订阅时拒绝删除。切换脏表单、删除和关闭未保存表单均需确认。

订阅的导出原文、启用/停用和设为当前不提交或重置编辑表单；刷新读取已保存的 URL，
只更新成功缓存与只读原文，不采用未保存 URL，也不覆盖名称、URL、UA 或覆写绑定草稿。
操作进入等待态时立即通知视图，禁止新建、切换选中项或更改绑定，结束后恢复操作；
取消脏表单切换应保留完整草稿。订阅页的 Ctrl+S 只保存已有订阅，新建的“添加并刷新”
仍须显式点击，不通过保存快捷键隐式联网。
Windows 原生已用回环订阅夹具复现导出丢稿，并验证修复后的导出、设为当前、启停、
延迟刷新及 Escape 取消切换均保留四项草稿；延迟刷新期间选择操作不可调用。

“预览”按共享 v2 顺序合成当前订阅节点、本地模板、结构化 slices、远程/本地 YAML、远程/本地
JS、local override、面板特性和规则集物化结果。预览只读，补齐实际引用的缺失规则集并调用选定
核心的真实 `check`；启动不复用预览文本，而是重新生成、再次准备资源并执行同一能力边界和核心
校验。订阅、覆写、结构化配置或规则保存会使预览重新生成；并发变更排队一次后重跑，避免旧预览
重新开放启动按钮。Ctrl+R 可手动重新生成。保存成功不热应用，运行和配置操作期间订阅、覆写、
结构化配置、规则及预览锁定；任一业务草稿未保存时拒绝启动，避免误把已保存旧值当作当前表单生效。

启动前资源准备使用一次保存的请求策略快照，预览与真正启动行为一致。保存为 Core 策略时，
尚无就绪自有核心，准备链只读取已有缓存，不探测或借用保存端口，不自动直连或启动临时核心。
所选订阅没有节点缓存时明确阻止准备，提示显式选择其他请求策略获取订阅或导入缓存；
远程 YAML/JS 覆写保留可用缓存，可选覆写及规则缺失继续告警降级，手写规则可从保存内容恢复。
其他显式策略仍可准备资源；GPUI 的覆写、自定义规则与内置物化请求共用固定策略，不再使用
内置下载器的强制直连例外。旧 Tauri 准备入口的网络行为不随此变更。

内置规则集只为当前仍声明为 remote 的条目准备旧式文件；已提供 local／inline 定义时不额外
下载另一份缓存，也不因旧式缓存不存在而删除有效路由或 DNS 引用。未声明且未引用的内置项
不报告缺失。真正缺失的远程项与未定义引用仍遵循原有降级规则，不把缺失资源伪装成可用。
上述范围限于应用侧准备；其他保留的 sing-box 远程规则集仍按核心自己的配置和缓存处理。

共享进程恢复使用 OS 创建身份与可执行路径，拒绝 PID 重用或身份不可证明的记录。macOS
保留身份采集及自有 `Child` 停止；自动终止存活孤儿进程暂不支持，保留记录并明确失败。
不能用 PID／名称强杀来绕过该边界，也不能把交叉编译检查当作 macOS 原生运行验收。

Windows 原生回环验收已覆盖上述准备链：真实空目录由 GUI 建立身份后，缓存场景在保存
Core 策略且外来监听占用保存端口时仍可预览、启动；准备期间未新增订阅或覆写请求，
也未连接外来监听。实际代理流量证明保留的本地 private 规则可直连允许目标，
后续拒绝规则阻止另一目标抵达源站。缺少必要订阅缓存的独立根明确显示失败且不能启动。
两个场景均正常关闭 GUI、退出码为 `0`，未触发超时或强制清理；普通核心退出码 `1`
只证明该自有进程已退出，不代表托管 TUN 的优雅停止分类或系统恢复已验收。
共享定向回归覆盖缓存保留、固定自定义代理快照、手写规则恢复及未开始请求不计刷新年龄。
本批集中完整 Rust 检查通过：workspace 构建、fmt、严格 Clippy、888 项单元测试、
3 项集成测试、1 项文档测试，以及 Tauri host Clippy 和 Android target check；
显式忽略的副作用用例未执行，原有 Android 条件编译警告未抑制。Windows 原生定向测试、
GPUI 严格 Clippy 与构建通过；两个 macOS target 的共享 common/engine 测试代码
通过严格 Clippy，但没有 macOS 原生运行证据。本轮不操作系统代理、TUN、UAC 或 CA。

“网络与系统”保留 Clash API 的端口、密钥和面板选择；关闭 API 不清空这些值，关闭时允许保存
端口 `0`，混合代理端口始终要求正数。重新启用 API 必须先设置有效且不与代理或已启用 MITM
回流端口冲突的端口；保存失败保留草稿和旧文件，不部分写入其他设置。共享合成在关闭 API 时
移除整个 `experimental.clash_api`，包括模板或先前合成的面板目录和下载地址，不能通过模板重开
控制面。此时连接读取/关闭及在线策略操作不请求 API；重新启用后复用保留的密钥认证。GPUI
仍不生成外部面板资产，现行 Tauri 的面板打开及复制入口也检查已保存的 API 启用状态。

“代理策略”展示运行核心返回的全部组和成员；Selector 成员确认后先切换核心，再写入实验
`app.toml` 的 `[client]`，下次启动等待 Clash API 就绪后逐组恢复有效选择，失效成员不阻止核心启动。可对所选
成员或当前整组测速；超时显示为无延迟结果，读取/操作失败保留最后成功快照并允许重试。

规则/全局/直连模式可在停核时保存，生成配置的 `default_mode` 使用该持久值；运行中切换还会
热推送 `/configs`。保存成功但热推送失败时明确保留“已保存、当前同步失败”状态，下次启动再次同步。
这些操作只影响 GPUI 保存的 loopback 控制面。系统代理与 TUN 分别由显式设置及所属会话
控制，停止或恢复不能无条件关闭第三方代理、删除仅凭名称匹配的网卡。

“结构化配置”直接编辑共享 `ConfigSlices`：DNS 跟随系统/接管、服务器、规则和探测，自定义节点、
Selector/URLTest 分组、内置组可调项、默认路由/域名解析器及缓存文件。数值输入保留无效原文并
阻止保存或切换选中项；删除服务器、规则和自定义出站需确认。一次保存先执行跨切片引用与协议
字段校验，再原子替换 `config_slices.json`；失败保留草稿和上一份文件。DNS 与出站引用使用实际
sing-box tag，内置分组名称、协议和动态成员仍由共享模板规格维护。DNS 探测只请求当前服务器，
不修改持久配置；结果或错误显示在同页。

页面工具栏整合分段导航、保存与重新载入；模式/协议/策略使用紧凑分段按钮，启用、反向映射、
TLS、缓存及中断行为使用 Switch，不再把状态开关渲染成命令按钮。新增出站采用页内渐进展开的
类型选择，创建后收起并进入未保存状态；编辑、校验、保存和状态文案沿用同一草稿与错误归属。

“规则”直接编辑共享 `LocalOverride`：路由规则支持全部 match/action/advanced 字段、顺序和
多规则集引用，第一个启用的 Final 规则覆盖结构化默认路由。页面工具栏整合路由规则、规则集与市场；
匹配/动作/来源/市场筛选使用紧凑分段按钮，规则启停与批量分类使用 Switch，排序使用带无障碍标签的图标按钮。
新建规则或规则集先显示标准确认框，取消或关闭不修改草稿，确认创建后才进入未保存状态。
规则集支持远程 source/binary、手写 source、分类批量整理、单项/全部智能更新和显式恢复内置项；删除规则集后失效引用会保留并提示，
不静默改写规则。市场从现行 MetaCubeX 目录源生成 geoip/geosite 条目，可选择 GitHub Raw 或
jsDelivr。一次保存原子替换完整 local override，但规则集更新只合并后端检查状态，不覆盖并发分类
编辑或复活已删除项；预览和启动准备实际引用的缺失 backing files。

“脚本”以 Surge 为主要、Quantumult X 为次要兼容目标，仅开放这两种方言的新建、切换和配置导入；
Loon、Stash 选项继续显示并标明暂不可选。历史清单保留真实方言，允许不改变方言的编辑或用户明确
选择受支持方言，不删除、自动迁移或伪装成其他格式；选择本身不转换配置内容。URL 嗅探仍报告真实
方言，新记录若检测为未开放方言则阻止保存。界面、动作处理和 GPUI 业务入口使用同一选择策略；
共享解析器、历史脚本宿主及 Tauri/Android 的能力不因该界面策略被移除。

**首个非 TUN RC 的有限脚本承诺：**

| 层次 | 支持范围 | 明确排除 |
| --- | --- | --- |
| Surge 明文导入 | URL Rewrite 的 header/302/307/reject；Header Rewrite 的 add/replace/del/replace-regex；Body Rewrite 有序替换；Map Local 内联内容；Script 请求/响应/cron；MITM hostname | 本地/外部文件、jq、未知脚本类型/引擎/选项，不是完整 profile 转换器 |
| QX 明文导入 | rewrite_local 重定向/reject 系列、请求/响应头与正文替换、四种 header/body hook、echo/analyze-echo；task_local cron；hostname | url-and-header、外部 echo 文件、jsonjq、递归资源组合与 `#out` 选择 |
| Hook 完成 | 首条 enabled 匹配 hook；当前请求/响应上下文；url/headers/body/bodyBytes、响应状态、Surge 嵌套 response、QX echo 平面响应、abort | 未知完成字段拒绝 mutation；URL 须为绝对 HTTP(S) URI，非法值不能只拒绝 URL 却部分应用头/正文 |
| Surge 宿主 | `$httpClient` 已实现的 HTTP 方法/回调、二进制与重复头选项；`$persistentStore`；宿主通知转交 | 私有策略选择、任意厂商 HTTP/通知选项、完整环境版本身份 |
| QX 宿主 | `$task.fetch` Promise、已实现的重定向/cookie/TLS 选项、文本/字节响应；`$prefs`；通知转交 | 不冒充 Surge API，不承诺未实现的 timeout/策略/厂商选项 |
| 执行与任务 | QuickJS、`$done`/参数/现有元数据、Promise/setTimeout、完成/取消屏障、5/6 字段 cron 与手动执行 | 不是 Node/浏览器/JSC；不承诺 Surge/QX interval/clearTimer 扩展、wake-system 或真实账号服务等价 |
| HTTP/MITM 与缓存 | 已批准 hostname/端口及既有 CA 下的显式代理；缓存后按启动快照执行；下文正文/正则预算及参数展开 | 不是 TUN/透明代理、任意 SNI/Host 互换、私有证书导入或无限缓冲；缓存刷新不热替换当前会话 |

脚本异常/超时沿用 hook fail-open，未提交的 mutation 保持原流；这不同于正则预算失败终止
交换及 required 请求正文不可检查时拒绝出站。Continue 只说明继续转发，不证明 JS 成功。
不承诺加密容器、远程 eval、厂商资源解析器/面板/API、账号支付签到、protobuf/字幕服务、
地域解锁或整包递归合成。已有 Loon/Stash 历史能力不是本 RC 新增开放范围。

支持清单 CRUD/启停、URL 嗅探、头部元数据、模块参数、立即更新、配置导入以及 cron 任务状态和手动运行。
GPUI 每五秒检查已启用资源是否到期，页面未选中时同样有效；首次无刷新记录的资源也会自动获取。
未保存业务草稿、核心/备份/更新等生命周期或其他业务操作期间延后。健康自有核心运行时仅更新
下次启动使用的缓存，不热替换当前核心、MITM、规则或 cron 会话；不自动停核或重启。
手动“立即更新全部”仍强制更新已启用资源，并重置其下一次自动尝试时间。
共享服务将已完成尝试的时间原子保存在 `remote_refresh.json`，按名称、URL、类型和方言的哈希身份
归属，重开后保留；时钟回拨不永久冻结调度，零间隔沿用既有的每日归一化规则。
失败按所设间隔等待下一次尝试，不额外密集重试；取消中的资源和未开始资源不记作完成。
每批自动下载固定使用一份已保存的请求策略快照，包含模块依赖，不在下载中途重读策略。
核心代理只接受当前会话证明拥有且就绪的实际监听；本应用的系统代理或自定义 loopback 代理
改走自有核心回流端口，避免再次进入本应用 MITM。显式 HTTPS 或带凭据的自有代理不能降级
为无认证 HTTP，拒绝该轮更新；第三方自定义代理不改写。停核时核心代理不可用即报错，
不借用保存端口上的外来监听，也不静默直连。进程创建身份和监听归属在请求、重定向、
重试及发布前复核；自有核心失效时取消下载并保留旧缓存。
启动时按配置类型记录核心仍可能读取的文件；到期批次的发布目标若与其重叠，整批在首次
请求前延后，发布时再次核对路径别名与链接变化。缓存更新不重建当前调度器或 MITM。
手动、自动、清单和导入写操作共享同一写锁，自动遇到占用则延后，不能与手动更新并发发布。
自动单资源下载（含必要脚本依赖）限定三十秒；失败、空模块响应或必要 hook/task 资源缺失时保留
上一份完整模块，不把所有解析警告都当成失败。装饰图标不占用自动下载预算，由保存或手动更新维护。
清单、缓存及刷新记录原子发布；损坏的刷新记录不会默默重建，自动/手动更新均报错且保留原文件。
关闭或显式启停等互斥生命周期先取消自动网络工作，等真实 worker 退出后才进入核心操作或释放目录；手动已接受操作仍等待
完成。后台结果不替换编辑框、选中项或未保存值；无到期资源的轮询不覆盖上次报告。此自动定时入口
仅接入 GPUI，Tauri/Android 的手动共享服务继续可用，不据此宣称其后台调度或设备行为已验证。

前一批停核场景已在 Windows 原生独立目录验证：未打开脚本页时获取模块、到期替换缓存、未保存的真实编辑草稿
暂停请求且不被覆盖、显式保存后恢复更新、HTTP 503 在页面告警并逐字节保留旧缓存。自有服务器
挂起响应时正常关窗取消连接，本用例约 70 ms 退出且返回 0；缓存及完成年龄没有延迟发布。
服务端仅接收自有回环请求，测试目录由真实应用先认领，不伪造目录标记；未生成 CA、启核或改系统网络。
该批次共享服务的八项定向回归在 Linux/Windows 通过，集中 Rust 门禁含 Tauri host/Android target、
GPUI 原生严格 Clippy 和 Debug 构建通过；其结果不包含运行核心、提权、Android 设备或新签名发行包。

本次扩展已在普通权限 Windows GUI 与真实独立 sing-box 实例通过：模块及必要任务脚本经该核心
的实际 TCP 连接获取，服务端回读客户端连接 PID，证明并非 GUI 静默直连。v2 缓存发布后当前
任务仍执行 v1，显式停核再启动才执行 v2；活动核心配置与保存的客户端配置摘要不变。
挂起模块下载时显式停止，本用例核心约 126 ms 退出，真实 TCP 随即取消；再观察七秒，
缓存及刷新年龄均未延迟发布，也没有停核后的外来端口借用或直连请求。GUI 正常关闭返回 0，
看门狗及失败强制清理未触发。此结果不构成核心优雅退出分类、TUN、系统代理或系统 CA 验收。
夹具显式使用无外部规则集依赖的直连配置；冷缓存规则补齐与全部请求代理组合仍须分别验收，
不能将停核后的预览资源准备耗时混入实际核心退出耗时。
刷新定向回归为 Linux 十七项、Windows 十六项（路径链接回归仅适用 Unix）；下载跨越其他资源
到期时间的用例已先失败后通过，固定批次准入与执行集合。集中门禁通过 workspace build、
fmt、严格 Clippy、887 项单元及 1 项文档测试、Tauri host Clippy 和 Android target check；
四项需单独环境的集成/文档示例保持显式忽略。GPUI 原生严格 Clippy、端点/输入清单回归及
Debug 构建通过；Android target 仍有两项既有条件编译警告，不代表 Android 真机验证。

远程资源/导入草稿会阻止核心启动；运行中锁定清单和导入写操作，但允许查看及手动运行已启动
调度器中的任务。核心停止和正常关窗会等待调度器退出。兼容目标只承诺已验证的公开子集，
不等于完整厂商客户端、资源解析器、管理 API 或 WebView 引擎支持。

Windows 原生脚本业务回归已验证未开放方言不能新增、替换原方言或写入导入缓存，失败保留旧数据；
历史 Loon/Stash 记录可保留原方言编辑，也可明确改为 QX，Surge/QX 导入仍可激活规则。
真实窗口已验证两组方言选项可见、未开放项无激活动作，点击后 QX 仍保持选中；
回环 `.plugin` 嗅探保留 Loon 并禁用添加，明确选择 Surge 后恢复添加动作。正常关窗且未生成 CA、
启核或改变系统网络。该批次使用定向 Windows 测试、严格 Clippy 和 Debug 构建，不重复全量根/Android 验证。

任务以模块、来源和声明名的无歧义独立标识注册；列表仍保留声明显示名，手动执行、定时执行、
移除和运行记录均使用标识，不再以显示名查找或互相覆盖。源与模块元数据、参数和
`$script.name` 分别传递，不用调度标识替代脚本可见名字。无法激活的缓存保持停用，
启动日志保留重新获取或导入的恢复诊断，不把所有注册错误都称为 cron 语法错误。

脚本定时器按方言提供公开行为：[Stash](https://stash.wiki/en/script/syntax-and-interface) 的
`setInterval`、`clearTimeout`、`clearInterval` 使用真实运行期句柄，可取消已到期但尚未执行的回调，
也可在 interval 自身回调中停止后续触发；[Loon](https://nsloon.app/en/docs/Script/script_api/#23-setintervalcallback-delay-args)
的 `setInterval` 保留官方规定的单次行为。`$done` 在下一次 timer 回调前形成完成屏障，
不是等整批到期回调执行后才检查。真实 `ScriptWorker` 与磁盘存储重开已验证晚写入不会越过完成屏障，
不据此宣称全部厂商私有 API、通知附件或 Windows 系统接管已兼容。

Stash 的 `queueMicrotask` 与 Promise 使用同一真实 QuickJS job 队列，按 FIFO 执行，
并在下一次到期 timer 前完成微任务检查点。`$done`、取消与截止时间同样约束微任务、
HTTP 回调和存储/通知等宿主副作用；回调异常立即返回原始错误，不伪装为超时或成功。
JS job 的调度闭包留在可由 JS GC 追踪的作用域，Rust 闭包不长期捕获 JS 函数/上下文。
独立进程已验证连续执行、Worker 关闭后磁盘重开及 Stash 写入由 Quantumult X 读取；
其他方言在 QuickJS full context 中原有的 `queueMicrotask` 仍保留，不称为 Stash 独占 API。

“MITM”复用脚本缓存中的 rewrite、hook 与派生 Hostname，支持开关、手动白名单、隔离 CA、按当前
CA 指纹查询系统信任、显式确认安装、PEM 复制和进程内抓包记录。手动 IP 白名单按精确 `/32` 或 `/128`
路由，域名继续按完整域名或 `*.` 后缀匹配。启用且存在有效白名单时，核心启动动态增加
`main-in`、`lanpath-mitm` 与 `mixed_port + 1` 的 loopback 回流入站（默认 `127.0.0.1:17900`）；空白名单保持普通单入站。
CA/私钥只写实验根，页面不会自动安装 CA、设置系统代理或启用 TUN。验证时不得为了通过测试直接
触发“安装到系统信任库”；系统证书变更仍需用户单独授权。

Surge 的正文行为遵循公开[请求](https://manual.nssurge.com/scripting/http-request.html)与
[响应](https://manual.nssurge.com/scripting/http-response.html)契约：需要正文的请求超限或无法完成
有界检查时，在出站前拒绝；响应正文超限则跳过脚本并保留原始流。脚本检查独立于重写和记录的
128 KiB 上限，Surge 默认采用桌面的 10 MiB 策略，显式检查范围仍限定为 32 MiB。
超过该范围的数值声明不激活；响应 `max-size=-1` 会完整说明本地仍有 32 MiB 检查范围（兼容提示，GPUI 本地资源识别与保存不因此拒绝已支持声明），
更大响应透传，不宣称真正无限缓存。`$script.binaryBodyMode` 来自实际执行配置，
`$request.id` 在成对 hook 间保持一致，并与捕获记录使用同一请求身份。
真实宿主已验证同名模块分别执行、二进制模式/请求标识和请求溢出拒绝；
这不替代 Windows 原生导入到被拦截 HTTPS 的完整验收。

CONNECT 还原的 URL 在重写、hook 与记录前统一省略 `http:80` / `https:443` 默认端口，
避免常见 `^https://host/path` 规则失配；非默认端口、IPv6 字面量和已转义的路径/查询字节保持不变。
真实 TLS/QuickJS 回归覆盖四种方言、QX 回显及不受信任 CA 的拒绝，不安装系统根或关闭证书校验。
MITM 页面使用显式保存的复选框，不把下次启动生效的配置伪装成即时开关；运行状态切换后清除旧保存提示。

IP 客户端未发送 SNI 时保留已批准的完整 CONNECT Authority；有 SNI 时仍按原端口独立校验，
不接受未批准的绑定。签发证书按 IPv4/IPv6 生成真正 IP SAN，而不是同形 DNS SAN。
Windows 已由实际页面导入 Surge、Loon、QX echo 与 Stash，关闭资源服务器后从缓存经真实核心、
客户端严格私有根/主机校验与 QuickJS 得到各自参数对应的 HTTPS 本地响应。
Stash 导入明确报告 QuickJS 与 JSC 的引擎差异；此路径不是实际源站 HTTPS 转发验证，不安装系统 CA。

另一次 Windows 自有夹具已通过核心 mixed 入站、MITM 与真实源站访问
`https://1.1.1.1/cdn-cgi/trace`，取得 200 并校验真实响应结构；客户端仅信任该夹具的私有 CA，
同时校验主机名和服务器证书用途，不绕过 TLS 校验或写入系统信任。
自有回环 TLS 源站使用包含有效 IP SAN 的自签名证书；为避开私有 IP 的既有前置路由规则，
负例直接访问本次 MITM 端点。客户端前端 TLS 校验通过，MITM 返回 502，源站接受 TCP
但未收到 HTTP 请求。此结果证明该场景未向不受信源站发送 HTTP；未观测到具体上游 TLS
错误码，不扩大为全部证书拒绝边界的证明。随后正常停核、关窗，四个监听端口全部关闭，
原客户端配置与结构化配置按摘要恢复；未启用 TUN、系统代理或系统 CA。

在用户另行授权后，固定验收根的既有 CA 已仅安装一次到 Windows 当前用户 Root，并由产品所有权
回执绑定；系统级 Root 未写入。实际 Rules 的 TGR 模块/脚本经导入、缓存与真实 MITM HTTPS，
使用 Windows 默认系统信任得到准确重定向，空自定义信任库则拒绝证书链；未访问 Telegram 上游。
同一数据根保留 `certs/` 后复用身份，不按测试包重新生成或安装，不向其他独立根静默复制私钥。
这不改变证书安装必须显式授权的产品规则，也不意味着任意资源包、上游证书或厂商 API 已兼容。

正则由共享 `Pattern` 统一导入、缓存恢复、rewrite 和 hook 选择。普通表达式先用 `regex`，
必要时回退到 [`fancy-regex`](https://docs.rs/fancy-regex/0.20.0/fancy_regex/)；
前后向断言受该实现及本地预算约束，不等同于完整 PCRE 兼容，也没有引入 C 引擎依赖。
替换保持 `$1`、`${name}`、`$$` 和 UTF-8 空匹配边界语义；普通快路径不套用回退匹配次数上限。

| 资源 | 本地上限 |
| --- | --- |
| 表达式 / 委托编译 / 回退指令 | 16 KiB / 2 MiB / 16,384 条 |
| 回退匹配与替换输入 | 256 KiB |
| 普通替换输入 | 32 MiB |
| 每次回退搜索的回溯 | 20,000 步 |
| 一次回退替换 | 256 次匹配，迭代输入长度累计不超过 8 MiB |
| 替换模板 | 16 KiB |
| 替换输出 | 不超过 32 MiB，且增长不超过 8 MiB |

这些是资源预算，不是硬墙钟超时。预算或匹配错误脱敏返回并终止当前交换，不能作为“未匹配”
继续转发部分改写，也不能把响应失败变成空 200。请求/响应正文上限仍按各自既有策略生效。

MITM 的 `{参数}` / `{{{参数}}}` 声明保存原文，每次加载按当前已声明默认值及用户值展开。
键和值先分开再展开，不重新解析成配置；`#` 等注释键停用该声明，未解析参数、控制字符、
段名或证书安全选项注入不激活并诊断。IPv6 方括号必须对应有效地址，域名端口与排除项保留。
运行白名单、来源列表和导入/更新的域名统计采用相同参数语义；停用后再启用不冻结旧解析值。
GPUI MITM 页每次进入时加载一次保存的聚合结果与证书状态，停留期间不轮询静态设置；
核心启停和 CA 操作仅更新对应状态，请求记录仅在核心运行且记录分区可见时每 2 秒读取，结果不变不通知页面；
派生主机更新不覆盖手动主机和开关草稿。资源与参数修改仍在下次启动时应用于运行链，
不从脚本匹配表达式推导未声明主机；只有排除项不满足启动的包含项条件。
旧 v2 缓存继续可读，但之前已被旧引擎丢弃的规则和模板不会凭空恢复，需要显式重新获取或导入。

本批次在 Windows 原生临时数据目录中完成以下实际 Rules 文件的导入、磁盘缓存与执行烟测：

| 资源 | 已观测结果与边界 |
| --- | --- |
| Surge TikTok AT / QX TikTok JP | 分别保留 4 / 6 条重写；区域断言经真实 MITM HTTPS 返回准确 307，未信任该 CA 的隔离客户端拒绝连接；未访问 TikTok 上游或验证服务解锁 |
| QX Youtube_CC | 两条负向断言头改写分别验证添加目标语言、排除已有语言及保留其他头；两条资源链接仍诊断不支持，不宣称完成厂商 `#out` 选择或整包组合 |
| Surge YouTube-ZH | 两个 hook 从真实本地源缓存恢复；原始请求清理 JS 经 QuickJS 删除 Cookie 并保留无关头，响应 hook 排除 `_ytzhsub=1`；未执行远程字幕翻译 |
| Surge WPS | 原模块解析及默认域名激活通过，未获取或运行账号/cron 脚本；只提取参数与 MITM 声明作受控远程夹具，停止服务器后离线反复启停，来源与运行域名一致 |

烟测复用同一既有 CA、显式加入隔离 Rust 客户端的信任，证书文件及 Windows 信任基线未变；
不是本批次重新证明默认系统信任，也不重新生成或安装 CA。端到端烟测设 45 秒总截止时间、
单请求 5 秒超时，正常关闭代理与自有服务器；上游接收连接数为零，未启动核心、TUN 或系统代理。
本批共享 Rust 门禁含 Tauri host 与 Android target 检查通过，Windows 定向回归、严格 GPUI
all-targets Clippy、Debug 构建及受控烟测通过；不等同于新签名发行包或 Android 设备验收。

RealIP 自动解析只对 MITM 白名单的精确补集执行；仅当相应无附加条件的终结模式路由已位于
MITM 前面时，直连/全局模式才继续强制解析。不会从解析条件中额外注册用户未声明的模式。
主入站命中且未排除的域名可在无上游 DNS 时执行本地响应；回流、未命中、端口不匹配和排除项
仍按正常 DNS/路由处理，显式 `resolve`、DNS 接管及 FakeIP 不被重写。规则优先级保持不变；
尚未解析的域名不能在进入 MITM 前按 IP 规则分类，实际转发须在回流路径解析并再次分流。
Windows 真实核心已验证本地响应无需源站 DNS、返回路径/排除项的正常解析、IPv6/本地拒绝、
模式切换，以及用户自有模式不被额外注册；此回归不安装系统 CA、不启用 TUN 或系统代理。



左侧按“配置 / 运行 / 扩展”分组导航。“连接”使用同一后台 tracker 的活动/关闭快照和字节总量，
按连接 ID 保留选择，详情提供 IP、出站链、规则、字节和开始时间，可关闭所选活动连接。
“统计”可在停核或重开后查询：日聚合支持本地日期、搜索、精确出站和全部排序字段，关闭明细
支持搜索与全部排序字段；均可设置升降序和 1–1000 条返回上限。明细 API 没有日期边界或精确
出站条件，不以 UI 假过滤冒充后端能力。统计行保持单行预览，选中后可在只读详情中查看和复制
完整字段；选择按聚合复合键或明细主键保持。清空确认作用于全部实验聚合/明细，不限当前筛选；
清空会等待已接受查询完成并使旧快照失效，运行中的跟踪器仍会采集新数据。

统计条件与结果拥有独立视口，筛选区不能因自动高度塌陷而消失。输入错误不提交新查询，
也不清除上次成功结果。Windows 原生已通过实际输入和应用查询，使用隔离 SQLite 中回放的
既有记录验证历史日期/搜索/精确出站命中、详情展示、未匹配出站为空，以及无效日期按 Enter
后报错并保留原结果；此查询验收未启动核心，不代表本轮新流量采集。

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
实验 `app.toml` 的 `[client]`，不是加密凭据库。重载及关闭脏草稿需确认，正常关窗等待已接受的测试完成。

“外观”将 `theme`、`accent`、`pure_black`、`scale_percent`、`material` 和 `acrylic_opacity_percent`
原子保存到 `app.toml` 的 `[appearance]`，不写核心配置或读取现行 WebView 存储。主题支持浅色、深色及跟随系统；强调色为品牌蓝、鸢尾紫、
湖水青、玫瑰粉，默认跟随系统和品牌蓝。纯黑仅在解析为深色时生效，切换浅色保留选择。
缩放为 75–150 的整数百分比，通过主题基础字号和 rem 同时缩放控件、文字与间距，不只放大文字。
材质为 `acrylic` 或 `solid`，不透明度为 20–100；旧文件缺少字段时分别默认 `acrylic` 和 85，不需要改写旧 JSON 或迁移。
缩放、材质与不透明度为本机偏好，跨设备备份恢复保留本机值。启动首窗前应用保存偏好，跟随系统由保留的窗口外观订阅更新。选择先保存成功再应用，不显示常态已保存提示；读写失败
明确显示错误，损坏文件不自动覆盖，修复后重启应用恢复。

“核心”管理使用共享 `CoreManagementService`，按核心类型管理 `cores/<type>/<version>/`。页面标题行内嵌
紧凑分段式核心类型切换器，下方依次为当前核心卡片、紧凑稳定/预发布切换器和单个版本信息卡片；
当前核心卡片展示真实 `Environment` 与编译 `Tags`，本地版本在版本卡片内使用/删除，远程 release tag
只作为同一卡片更新信息。
类型切换不隐式下载、选择或重启核心。下载复用已保存的请求策略与 GitHub 镜像；解压至隔离 staging，
版本探测精确匹配后才发布并原子合并 `core_binary`。失败不改变原选择或覆盖既有核心；旧未完成版本
目录可补齐，但不删除其中其他文件。staging 不作为已安装库存。选择仅接受当前类型库存下无链接且
可执行的二进制；删除需确认，且不能删除当前选中项。运行或启停、其他配置操作、诊断及退出期间
锁定核心修改。下载只有连接和元数据请求超时，没有归档传输总超时；正常退出等待已接受操作结束。

Mihomo 稳定版使用 `v<semver>` 发布，预发布使用 `Prerelease-Alpha` release 的 `version.txt` 得到
`alpha-<hash>`；非 Windows 的 `.gz` 按单文件解压。Clash YAML 订阅直接复用原始 proxies，其他格式
通过双向节点转换；不能表示的协议明确报错。Mihomo 使用 `-d/-f` 启动和 `-t` 校验，sing-box 保持
`run -c -D` / `check -c -D`。Android 拒绝 Mihomo；Windows 受限 TUN helper 只接受 sing-box，
Mihomo+TUN 在准入处拒绝。sing-box 配置切片、本地规则和 `.srs` 规则集不应用于 Mihomo，存在非默认
配置时记录告警；Profile YAML/JS 覆写仍按各核心 schema 生效。

核心管理按“核心类型 + 通道”保留当前版本和一个上一版本备份。核心选择持久化到 `app.toml` 的
`[core]`、`[core.current]` 与 `[core.previous]`，包含核心类型、通道、版本和路径；实验数据根内的
核心路径保存为相对路径，加载时按当前数据根解析，避免绑定 `%LOCALAPPDATA%` 或某个用户名；
旧的 `client.core_binary` 在迁移会话中转换。受信种子仍供首启和受限 TUN helper 使用，但核心页
不再提供手动“安装内置核心”入口。

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

“关于”只显示真实构建版本、OS/架构与程序身份，移除独立目录、所选核心卡片和底部权限长说明，
不再读取库存或提供重新载入核心信息。核心状态和管理保留在窗口底部状态栏与核心页。

配置侧栏跟随持久选中的实际核心：sing-box 展示覆写、结构化配置和预览；Mihomo 暂只展示
预览占位。核心管理页切换浏览类型不切换配置菜单。原“系统网络”和“规则”统一收进
sing-box 结构化配置的网络与规则分区，DNS、出站、路由与缓存仍保留；Ctrl+S 保存当前分区，
切换分区保留各 feature 草稿，并继续拦截无效切片输入。当前只整理 UI，网络参数仍沿用
原共享存储与运行时行为；后续 Mihomo 编辑功能须单独设计配置存储和校验，不将菜单隔离
表述为已完成持久化隔离。详见 [桌面设计](desktop-design.md)。

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
实验目录独占租约并处理中断事务，再进入迁移会话；检测到待恢复包时由用户确认，依次校验备份、
迁移当前设置并原子恢复，成功后创建业务和统计存储。普通第二实例仍直接拒绝，不等待或抢锁。
恢复到 `app.toml` 环境时替换 `[client]` 与 `[resources]` 并写入独立 `subscriptions.toml`，保留本机 `[appearance]` 和核心选择，按目标布局映射缓存与本地导入；校验、迁移或恢复失败可返回
应用，待恢复包与当前数据保留。恢复保留本机所选库存核心，系统代理/TUN/MITM 不会自动开启。
启动结果及失败需要显式确认；成功重试清理旧失败标记，中断与拒绝事务保留既有恢复语义。

外观只迁移共享协议中的主题、强调色、纯黑模式，本机缩放不变；不支持的强调色保留本机值并
在预览和恢复后说明。损坏的本机外观文件不自动覆盖，保存失败保留恢复摘要和明确警告。
正常关闭等待已接受的文件选择、加密/解密或准备操作完成，不把后台工作留给已退出的窗口。

生成配置采用已保存的端口与 API 启用状态；默认 mixed 为 `127.0.0.1:17899`、
Clash API 为 `127.0.0.1:19090`，MITM 生效时另使用 `mixed_port + 1` 的 loopback 回流端口（默认 `17900`）。
占用则失败，不接管占用进程。TUN 仅通过 ADR-0014 的受控核心会话启用，系统代理采用显式
保存的所属会话设置；仍拒绝系统时钟改写、额外监听及未审定能力。资源受所属数据根与冻结快照
约束，不接受环境变量展开或外部 UI/自动证书资源。文件日志和缓存输出分别限于根下 `logs/`、
`cache/`。能力边界先检查，具体配置 schema 仍由同一真实核心负责。

可用 `LANPATH_TEST_SING_BOX` 显式选择真实核心，单独执行普通套件跳过的生成配置与生命周期回归：

```powershell
$env:LANPATH_TEST_SING_BOX = Join-Path $env:LOCALAPPDATA 'LanpathExperimentalCore\sing-box.exe'
cargo test --release --manifest-path apps/gpui/Cargo.toml --locked --lib business::tests::generated_subscription_pipeline_starts_real_core -- --ignored --exact
cargo test --release --manifest-path apps/gpui/Cargo.toml --locked --lib business::scripts_tests::fetched_remote_task_runs_while_real_core_is_live -- --ignored --exact
cargo test --release --manifest-path apps/gpui/Cargo.toml --locked --lib business::mitm_tests::generated_pipeline_records_real_mitm_traffic -- --ignored --exact --nocapture
```

Linux 主机构建不代表 Windows/ARM64 发布或 Android 设备验收；本阶段没有新增 CI 门禁。

第 12 单位的 [ADR-0013 设计草案](adr/0013-gpui-experimental-parallel-migration.md)已由
[ADR-0014](adr/0014-gpui-native-integration-and-distribution.md)明确权限与分发边界并进入实施；
实现状态与目标平台的行为验收分别记录，不以草案、构建成功或单项接管代替全量迁移验收。
系统代理应用与归还、UAC/TUN/路由/DNS、临时 CA 安装及移除分别需要明确许可，
并在无生产实例的专用 Windows 环境验证；发现生产代理或身份冲突应拒绝接管，
不能自行停止生产程序。源码调查和只读状态核验不能替代这些系统效果验收。

### GPUI ZIP 打包、验签与载荷恢复

以下工具只处理独立 GPUI 身份，不触发上传、发布或 Tauri 切换。先完成对应目标的真实原生构建，
并由受限环境提供既有 `GPUI_SIGNING_PRIVATE_KEY_FILE`；不得为排错生成临时产品签名身份或关闭验签。
`stage` 在计算摘要前通过 Windows SDK `mt.exe` 规范化副本；WSL 可通过 `GPUI_MT` 和
`GPUI_WINDOWS_TEMP_DIR` 指定已授权的 SDK 与 Windows 原生临时目录。不再依赖 NSIS 或生成安装器。

```sh
devenv shell -- bun apps/gpui/tools/package.ts all x86_64-pc-windows-msvc
devenv shell -- bun apps/gpui/tools/verify.ts verify x86_64 MANIFEST ARTIFACT
```

`all` 依次执行 `stage`、`portable`、`manifest`；也可单独执行各阶段。产物位于
`apps/gpui/target/TARGET/release/bundle/gpui/`：唯一 `-portable.zip`、
`gpui-ARCH.json` 及其签名、摘要清单。ZIP 同时适用于便携和标准目录，不再提供 setup 或第二套更新描述符。
ARM64 使用 `aarch64-pc-windows-msvc` 与验签架构 `aarch64`；交叉编译、PE 架构检查不替代原生运行。
参考 [RustGrid 的发行流程](https://github.com/yisier/rustgrid/blob/main/.github/workflows/release.yml)
按架构分发 ZIP；不照搬其单 EXE 归档或 UPX 步骤，Lanpath 还须携带已认证核心、wintun 和许可资源。

应用只下载已认证 ZIP 及原始清单/签名，不启动安装器或在线覆盖自身。退出目标程序后，
在 Windows 或能调用原生 Windows 程序的 WSL 中执行；目标、归档及外部 helper 均须在 Windows 本地卷，
不能在 WSL/UNC 源码目录执行载荷替换：

```sh
devenv shell -- bun apps/gpui/tools/verify.ts upgrade x86_64 MANIFEST ZIP ROOT
# 中断恢复：GPUI_UPDATE_HELPER 指向 ROOT 之外的完整已认证包中的 lanpath-gpui.exe
devenv shell -- bun apps/gpui/tools/verify.ts recover ROOT
```

目标必须已有完整已认证包且新版本更高；初次使用直接完整解压 ZIP，不经过安装事务。
原生入口仅为 `--lanpath-update-payload ROOT EXPECTED_BUNDLE_ID` 和 `--lanpath-recover-payload ROOT`，
在 GUI 与业务初始化前处理。只检查所选目录的精确资源，不枚举或结束同名进程；占用时保留现状并拒绝。
`.gpui-payload-*` 状态绑定目录、数据模式与便携标记/data 身份；中断可回退，外部修改或身份变化保留证据。
旧 `.gpui-installed-*`、`.gpui-portable-*`、`.gpui-update-*` 状态拒绝自动采用或清除。
未知文件、用户数据及标准目录不属清理范围；切换标记前必须完成待恢复事务。移除安装器不会移除 TUN 授权。

后续 `gpui-v0.2.x-rc.N` 标签只走 GPUI 桌面工作流并标记 prerelease，见 ADR-0014 的退役条件；
当前不发布、不提前删除 Android/Tauri 壳，也不把移动 GPUI 的框架示例视为本应用真机通过。

Windows x64 已实际验证签名 ZIP 默认便携启动、退出后移除标记再以测试专属 `LOCALAPPDATA`
启动标准模式、正常退出和两根隔离；便携数据全部字节保持，标记恢复为默认便携。
通用工具已实际调用原生 helper，拒绝同版本覆盖及空目标安装且保留原文件/数据。
原生默认库回归 133 通过、10 个显式设备/效果用例未执行；严格 all-targets Clippy 与 Release 通过。
Linux 主机载荷回归 21 通过、构建通过；独立应用的主机严格 Clippy 仍受既有 Windows-only
协议/配置项的 dead-code 报告阻挡，未抑制该报告。RC/稳定标签的发布脚本已用 mock `gh`
执行并检查分支，未上传产物或执行 GitHub 发布；ARM64 原生与真实历史版本升级仍未据此验收。


## Android

ADB 已连接设备的选择、按授权安装、日志采集和原生行为验收见 [Android 调试 skill](../.agents/skills/lanpath-android-debug/SKILL.md)。

在根目录执行 bash apps/client/scripts/build-lanpath-core.sh，生成未跟踪的 lanpathcore.aar。
进入 apps/client 执行 bun run tauri android dev --target aarch64，使用前端热更新。
编译 CLI 会重建被忽略的 tauri.settings.gradle 与 RustWebView JNI 胶水；不要提交本机生成路径。
不要跨检出共享 Android CARGO_TARGET_DIR；Tauri Kotlin 生成文件可能被旧构建缓存漏掉。
正式 APK 使用 bun run android:build。调试已有草稿的设备前先征得安装/重启许可。

## 工作流

`.github` 中的 YAML 文件统一使用 `.yaml` 扩展名。当前工作流为 `ci.yaml`、`windows-build.yaml`、`android-build.yaml`、`release.yaml` 与独立的 `gpui-release.yaml`。

| 类型     | 使用时机     | 发现与计划                               | 实现与验证                                 | 文档与最终复查                       |
| -------- | ------------ | ---------------------------------------- | ------------------------------------------ | ------------------------------------ |
| feature  | 新用户能力   | 读取领域、平台边界与 ADR，列验收         | 最小完整实现，相关 verify/gates 和行为验收 | CHANGELOG、架构/API 文档，复查范围   |
| bugfix   | 既有行为错误 | 复现、定位根因，列回归路径               | 最小修复、真实可用的回归检查               | 记录根因与验证边界，复查无旁支变更   |
| refactor | 结构改变     | 确认基线、行为不变目标                   | 分步重构、相关 gates                       | 更新路径/架构，核对输出语义          |
| docs     | 说明变更     | 核对实现和命令证据                       | 修改并验证链接/命令                        | 消除冲突，复查权威与措辞             |

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

GPUI 本地资源由 `LocalResourceStore` 管理，沿用 `resources/imports` 下 UUID 注册表和单文件资源包；片段原文、绑定 JS 与配套图标一并保存和备份，详见 [ADR-0018](adr/0018-gpui-resource-storage.md)。本地 JS 文件名只是引用标识，片段解析不会读取宿主路径或联网。远程导入仍使用既有解析限制；历史本地导入不伪造缺失原文。
