---
name: lanpath-android-debug
description: 在 ADB 已连接的 Android 设备上调试 Lanpath，选择设备、按授权安装启动、采集脱敏日志并验证 VPN 等原生行为。用于真实设备回归与故障复现，不用于发布或清除用户数据。
---

# Lanpath Android 设备调试

先读取 [AGENTS.md](../../../AGENTS.md) 和 [开发文档](../../../docs/development.md) 的 Android 与相关领域验收项。
本地命令在 `nix develop --no-pure-eval` 中执行；工具不可用时参考 [环境 skill](../lanpath-environment/SKILL.md)。
构建及静态检查参考 [构建验证 skill](../lanpath-build-verify/SKILL.md)，不重复定义工具链或签名流程。

## 确认设备与测试范围

执行 `adb devices -l`，仅选择状态为 `device` 的目标。没有设备、`unauthorized` 或 `offline` 时报告具体状态；授权弹窗需要设备持有人处理，不循环重连。
多设备且任务未指定目标时，请用户选择；后续所有设备操作显式使用 `adb -s "$device_serial"`，不默认选择第一台。
`device_serial` 为当前任务确认的序列号，不写入仓库。WSL 看不到 Windows 设备时先排查连接边界，不擅自重启 ADB server、切换无线调试或开放监听端口。

记录设备型号、Android 版本、连接方式、已安装应用版本与本次复现目标。可按需执行：

```bash
adb -s "$device_serial" shell getprop ro.product.model
adb -s "$device_serial" shell getprop ro.build.version.release
adb -s "$device_serial" shell dumpsys package "$app_id"
```

从当前 [Tauri 配置](../../../apps/client/src-tauri/tauri.conf.json) 与 [Android Gradle 配置](../../../apps/client/src-tauri/gen/android/app/build.gradle.kts) 确认 `app_id`，当前为 `com.ybakiame.lanpath`；测试变体以实际 applicationId 为准。
不要把完整 dumpsys 或设备标识直接贴入公开报告。

## 准备构建与安装

确认测试的是当前源码调试包还是用户指定的已有产物，记录版本和来源；不要用旧包验证新修改。
源码构建使用既有 AAR/Android 构建命令，按目标设备 ABI 选择产物；当前共用调试命令构建 aarch64，其他 ABI 先核对支持情况。
从本次构建输出确定 APK 路径，不假定某个缓存目录中的最新文件就是本次产物。

安装、停止或重启应用前检查本次会话是否已授权；已有授权覆盖时直接继续，不重复询问。仅请求调试 skill 不等于授权操作设备。
已授权覆盖安装可执行 `adb -s "$device_serial" install -r "$apk_path"`，优先保留数据。
若签名冲突、版本降级或 ABI 不兼容，保留错误并说明阻塞，不自动卸载、清数据或使用绕过选项。
不为设备测试生成或更换发布密钥，不默认使用发布私钥签署调试包。

需要启动时，从合并 manifest 或设备包信息确认 launcher activity 后，使用显式组件启动；不猜测 activity 名称。
VPN 授权、网络切换及代理启动按任务授权执行；系统授权弹窗可请用户手动处理，不伪造授权结果。

## 收集证据与复现

先记录复现时间和应用进程 PID。可用 `adb -s "$device_serial" shell pidof "$app_id"` 获取 PID，使用 `adb -s "$device_serial" logcat --help` 确认设备支持的过滤选项。
优先按 PID 采集有限时段日志；进程崩溃或重启后需重新获取 PID，不能把旧 PID 的空日志当作没有异常。
应用启动前崩溃时，可按需收集有限时间范围的 crash buffer 或相关系统标签；仅应用 PID 日志可能遗漏 VPN/系统错误。
不默认清空 logcat、抓取完整 bugreport、读取应用私有目录或导出用户配置。原始日志暂存于仓库外，分享前隐藏订阅 URL/token、节点凭证、个人路径和设备标识。

依据本次改动选择验收路径，不要求每轮执行全部场景：

- 首次 VPN 授权：使用符合条件的测试设备/现有状态；点击后出现授权、忙碌态跨页保留、重复点击不产生多个弹窗，取消后可重试。已授权设备不能宣称验证了首次授权，也不自动撤销授权或重装。
- 代理运行：启动阶段、实际联网、停止与再次启动；区分 UI 显示运行与真实流量成功，不只检查进程存在。
- 订阅或规则集：刷新、切换、启动后策略与节点一致，以及缺失文件时的准备阶段；使用获准的测试数据。
- 备份恢复：仅在明确授权的测试数据上执行，遵循原有完整替换、确认及回滚策略；覆盖安装保留数据不等于恢复测试无数据影响。
- 生命周期与界面：切页、前后台返回、错误提示和目标屏幕适配；需要截图或 UI 自动化时只使用当前可用工具，缺少能力时报告人工步骤，不额外安装第三方工具。

## 报告与结束

记录构建版本、设备/系统概况、前置状态、复现步骤、预期与实际结果、必要的脱敏日志摘要，以及未执行项。
分别报告编译、设备行为和网络结果；无法连接设备时只报告已完成的主机验证。
结束有界日志采集等本次临时任务；应用运行状态按测试约定处理，不默认停止用户代理。截图、日志、APK 和个人数据不入库。
