---
name: lanpath-build-verify
description: 根据 Lanpath 改动范围选择并执行前端、Rust、桌面壳和 Android 构建验证，说明 mock 与设备验证边界。用于开发自查、回归或构建排错，不代替发布流程。
---

# Lanpath 构建与验证

先读 [AGENTS.md](../../../AGENTS.md) 和 [开发工作流](../../../docs/development.md)，查看当前 diff 及任务涉及的调用链。
本地执行命令先进入 `nix develop --no-pure-eval`；环境异常时使用 [环境 skill](../lanpath-environment/SKILL.md)。
统一命令定义在 [nix/commands.nix](../../../nix/commands.nix)，验证内容以实际脚本为准。

## 按改动选择检查

| 改动范围 | 对应验证 |
| --- | --- |
| 仅 Markdown/skill 文本 | 核对命令、相对链接、触发描述及 diff；无需应用编译 |
| `apps/client` 前端 | `bun run --filter lanpath-app verify` |
| `packages/client-core` | `bun run --filter @lanpath/client-core verify`，并检查受影响 UI/应用消费方 |
| `packages/ui` | `bun run --filter @lanpath/ui verify`，并检查应用消费方 |
| 跨前端包变更 | `lanpath-verify-frontend` 覆盖上述三个包 |
| Rust，包括独立 Tauri 壳 | `lanpath-verify-rust`，即根 workspace build 加完整 `verify:rust` |
| Go libbox 桥、Kotlin 或 Android 资源 | Go 桥变更先 `lanpath-build-aar`；执行 `lanpath-build-android-debug` 验证 Android 集成，涉及 Rust 时同时执行 Rust 检查 |
| 工具脚本及其对应行为 | 选择关联的现有 Bun 测试；`lanpath-test-tools` 覆盖当前共用工具测试集合 |
| Nix/工具链或跨平台构建边界 | 按开发文档及对应 ADR 验证受影响目标，不能仅凭环境求值成功或单个平台通过收尾 |

包间依赖和原生调用可能扩大范围，不仅按文件扩展名判断。共享状态、类型或 API 改动应追踪消费方。
前端 `verify` 为 build/lint/format，不是应用交互单元测试；仓库另有 Bun 工具测试，不应据此宣称前端应用已有单测覆盖。

## Rust 与 Android 的易漏项

[Rust 门禁](../../../scripts/check-rust-gates.sh) 包括根 fmt/clippy/test、独立壳 host clippy，以及具备 SDK/target 时的 Android target check。
根 Cargo workspace 不包含 `apps/client/src-tauri`。`verify:rust:fast` 会跳过测试，不能当作完整 `verify:rust`。
检查输出中的 Android 跳过提示：进程退出成功不表示 Android 已验证。任务涉及 Android 时应补齐环境后执行，无法补齐则明确记录未验证。

Android Rust check 不覆盖 Kotlin/资源或 Go AAR 集成。Go 桥变更必须在当前检出重建 `lanpathcore.aar`，再编译 Android；不要复用旧品牌或其他检出的 AAR。
不要跨检出共享 Android `CARGO_TARGET_DIR`，也不要提交生成的 AAR、JNI 胶水或本机 Gradle 路径。
无签名调试构建足以检查编译集成；仅在任务需要正式打包时使用 `lanpath-build-android-release`，不为验证临时创建或更换发布签名身份。

## 行为验收与收尾

按任务读取开发文档中的领域验收项，以及关联 ADR、`docs/testing/` 记录；修改状态管理时同时读取 [状态规则](../../rules/client-state-management.md)。
浏览器 `?browser-debug=1` 可检查布局和 mock 交互，不能证明原生 IPC、VPN、TUN、MediaStore 或真实核心行为。
目标设备安装/重启遵循既有授权；未执行设备验证时明确列出范围，不用主机结果代替。
ADB 已连接设备的操作与证据采集见 [Android 调试 skill](../lanpath-android-debug/SKILL.md)。

失败后先定位对应层并修复，按新改动或失败范围重跑；不要用反复全量构建替代诊断。
最终报告执行的检查、结果、跳过项及原因、设备/mock 边界；行为变化同步 CHANGELOG 和领域文档。
提交前检查 diff 不包含无关改动及生成产物。该 skill 不授予提交、推送、触发远程工作流或发布权限，按当前任务要求处理。
