# 开发与工作流

先 git submodule update --init --recursive，再 nix develop；根目录 bun install --frozen-lockfile。
本仓 flake/lock 独立，避免使用宿主工具链；Windows CI 的明确例外见 ADR。
前端运行 bun run --filter lanpath-app dev 或 dev:android。
浏览器使用 http://localhost:1420/?browser-debug=1；mock 不能证明真实 IPC/设备行为。
共享 UI 的 CSS 扫描由 apps/client/src/index.css 的 @source 覆盖。

## Android

在根目录执行 bash apps/client/scripts/build-lanpath-core.sh，生成未跟踪的 lanpathcore.aar。
进入 apps/client 执行 bun run tauri android dev --target aarch64，使用前端热更新。
编译 CLI 会重建被忽略的 tauri.settings.gradle 与 RustWebView JNI 胶水；不要提交本机生成路径。
不要跨检出共享 Android CARGO_TARGET_DIR；Tauri Kotlin 生成文件可能被旧构建缓存漏掉。
正式 APK 使用 bun run android:build。调试已有草稿的设备前先征得安装/重启许可。

## 工作流

| 类型 | 使用时机 | 发现与计划 | 实现与验证 | 文档与最终复查 |
| --- | --- | --- | --- | --- |
| feature | 新用户能力 | 读取领域、平台边界与 ADR，列验收 | 最小完整实现，相关 verify/gates 和行为验收 | CHANGELOG、架构/API 文档，复查范围 |
| bugfix | 既有行为错误 | 复现、定位根因，列回归路径 | 最小修复、真实可用的回归检查 | 记录根因与验证边界，复查无旁支变更 |
| refactor | 结构改变 | 确认基线、行为不变目标 | 分步重构、相关 gates | 更新路径/架构，核对输出语义 |
| docs | 说明变更 | 核对实现和命令证据 | 修改并验证链接/命令 | 消除冲突，复查权威与措辞 |
| adoption | 协议采纳 | 读 ADOPT，Pass 1 inventory/mapping/audit | 无决策冲突后 Pass 2 生成、协调、验证 | schema/conformance/项目检查与 report |

全部工作流按 AGENTS 完成定义验收；冲突、未经授权的架构/门禁/数据变更和未知验收标准
触发 AGENTS 停止条件。适用能力由项目事实决定，不生成无用途的 Agent/MCP/Prompt 工作流。
