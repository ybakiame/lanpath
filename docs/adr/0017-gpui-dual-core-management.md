# ADR-0017：GPUI 实验桌面双核心与版本管理

- 状态：Accepted（仅 GPUI 实验桌面；现行 Tauri 前端与 Android 仍使用 sing-box）
- 日期：2026-11-01
- 决策来源：用户要求核心类型切换、本地版本管理，并明确选择完整恢复 Mihomo 支持。

## 背景

客户端此前只支持 sing-box，并在设置加载、订阅转换和库存构造中主动清空 Mihomo 选择、删除
`cores/mihomo` 目录和剔除旧覆写模板。GPUI 核心页只有 sing-box 双通道和当前/上一版本展示，
没有核心类型维度；当前核心也不展示真实构建 Environment 与 Tags。Windows 的受信种子核心
同时承担首启与受限 TUN helper，不能把“移除一个失效的手动安装按钮”误扩展成删除该安全资源。

## 决定

### 核心身份与库存

- `CoreType` 包含 `sing-box` 与 `mihomo`；序列化同时接受旧 `singbox` 名称。
- 库存固定为 `data_dir/cores/<type>/<version>/`，本地选择仍只有一个全局生效核心。
- 核心类型切换只切换管理视图，不隐式选择版本、下载或重启核心；所有下载/使用/删除操作都
  锁定到当前管理类型。
- `app.toml` 的 `[core].type`、`[core.current]`、`[core.previous]` 为权威选择；数据根内路径
  继续保存相对路径。`client.json` 保留显式 `core_type`，并以实际二进制名称校验类型。
- 旧 Mihomo 配置、库存目录和覆写模板不再被删除或清空。

### 上游发布规则

- sing-box 使用 GitHub release tag `v<semver>` 及 `sing-box-<version>-<platform>` 归档，
  Linux/macOS 为 `.tar.gz`，Windows 为 `.zip`。
- Mihomo 稳定版使用 `v<semver>` 与 `mihomo-<platform-variant>-v<version>.{gz|zip}`；
  预发布使用固定 `Prerelease-Alpha` release，其 `version.txt` 资产给出真实
  `alpha-<hash>`，资产名为 `mihomo-<platform-variant>-alpha-<hash>.{gz|zip}`。
- 非 Windows 的 Mihomo `.gz` 是单文件压缩，不按 tar 归档解压。版本探测必须与选中的
  规范版本完全一致后才发布和选中。

### 管理与展示

- 核心页结构为：核心类型切换器 → 当前核心卡片 → 稳定/预发布版本切换器 → 单个版本信息卡片。
- 当前核心卡片显示实际类型、版本、`Environment` 与编译 `Tags`；本地版本在该版本卡片内切换、
  删除，远程版本只作为同一卡片的更新信息，不再使用“远端通道”卡片。
- 当前版本与一个上一版本备份的保留策略按“核心类型 + 通道”隔离。
- 移除 GPUI “安装并选中内置核心”按钮及其仅服务于该按钮的实现；共享种子释放、
  受信构建资源和 Windows helper 合约保留。

### 运行与能力边界

- 桌面共享 `ClientState` 和 GPUI 均按选中核心类型创建进程管理器，并分别使用
  `sing-box.json`/`sing-box.pid` 与 `mihomo.yaml`/`mihomo.pid`。
- Mihomo 启动配置由本地 Mihomo 模板合成；Clash YAML 订阅直接复用原始 proxies，
  sing-box/分享链接订阅通过双向节点转换生成 proxies。无法安全表示的协议必须明确报错。
- GPUI 预览和启动使用同一核心类型生成配置；Mihomo 使用 `mihomo -t -d <root> -f <file>`
  校验，sing-box 保持 `check -c ... -D ...`。
- 当前 sing-box 结构化配置切片、本地规则覆盖和 `.srs` 规则集不应用于 Mihomo；存在非默认
  配置时记录明确告警，用户可切回 sing-box。Mihomo 仍接受 Profile YAML/JS 覆写。
- Android 核心桥只接受 sing-box；Mihomo 返回明确不支持错误。Windows 受限 TUN helper
  只接受受信 sing-box 构建，Mihomo+TUN 在启动准入处拒绝，不进入提权路径。
- 现行 Tauri/React 前端本轮不增加核心类型控件，继续按 sing-box 展示；共享运行时的类型
  能力不构成 Tauri 产品界面已支持 Mihomo 的声明。

## 后果

- 核心管理从“版本列表 + 通道卡片”转为类型驱动的单一版本工作流，新增核心类型不再需要
  复制整页结构。
- Mihomo 用户可以管理并运行受支持协议的订阅，但 sing-box 专属结构化规则不会在 Mihomo
  静默生效；切换核心类型不会迁移或忽略用户已保存配置。
- 若后续要求 Mihomo 的结构化规则、Android 运行或 Windows TUN 对等，需要单独 ADR 和
  对应 helper/协议/能力策略实现，不能复用 sing-box 的校验器冒充支持。
- Windows 原生验收必须覆盖类型切换、Mihomo stable/Alpha 下载与真实启动、构建 Tags 展示、
  无内置按钮页面，以及 TUN+Mihomo 的明确拒绝路径。
