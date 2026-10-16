# 开发与工作流

先 git submodule update --init --recursive，再 nix develop --no-pure-eval；根目录运行 lanpath-install。
协议子模块为私有治理资源，维护者需有访问权限；普通公开源码构建不依赖该子模块，可跳过其初始化。
本仓 flake/lock 独立，避免使用宿主工具链；Windows CI 的明确例外见 ADR。
主 nixpkgs 使用 nixos-26.05 并由 flake.lock 固定提交；Bun 1.4.2 通过官方归档与 SHA256 固定，避免稳定源自带版本与 Windows CI 不一致。Rust 仍由 rust-toolchain.toml 固定；Android SDK 输入保持独立锁定，此阶段不更新其传递 nixpkgs。更新主源使用 `nix flake update nixpkgs`，随后执行双端完整验证，见 ADR-0004。
devenv 通过 Flake 模块接入，固定版本由同一 flake.lock 管理，不维护 devenv.yaml/devenv.lock。配置入口为 devenv.nix，工具、桌面、Android 和共用命令位于 nix/；可用 direnv allow 启用已提交的 .envrc。进入 shell 仅初始化环境，不安装依赖、不构建 AAR、不启动代理。
本地及 CI 共用 lanpath-install、lanpath-rust-targets、lanpath-verify-frontend、lanpath-verify-rust、lanpath-test-tools、lanpath-build-aar 和 lanpath-build-android-debug/release；这些命令可从仓库子目录调用，失败立即退出。仍可在 shell 中直接使用原 Bun/Cargo 命令。发布签名只由既有 release 工作流显式注入，Husky 继续作为唯一提交检查入口。
初次构建使用 cache.nixos.org 与 devenv 官方签名缓存；devenv 任务辅助程序有独立的上游锁定构建快照，不替换项目工具链，缓存未命中时可从源码构建。不承诺 Flake 集成具有 devenv CLI 的全部评估缓存和性能特性。见 ADR-0005。
前端运行 bun run --filter lanpath-app dev 或 dev:android。
浏览器使用 http://localhost:1420/?browser-debug=1；mock 不能证明真实 IPC/设备行为。
共享 UI 的 CSS 扫描由 apps/client/src/index.css 的 @source 覆盖。
桌面壳默认居中创建窗口，常规最小尺寸为 760×560 逻辑像素；启动与最小尺寸按主屏工作区和 DPI 收敛，保留边框空间。宽度低于 1024 时顶部提供主导航按钮，菜单支持键盘关闭与焦点恢复；侧栏直达页面保留标题，其他子页面返回显式 backTo 或路由父级。
代理启停和重启成功后共用 writeRuntimeStatus 取消旧状态轮询/运行查询，重置策略与活动连接缓存；启动后刷新已挂载查询，停止时不重拉核心数据。缓存回归用 `bun test packages/client-core/scripts/runtime-queries.test.ts`（在本仓 Nix 环境）执行，CI 同步执行；浏览器 mock 不等于真实核心验收。

## Android

在根目录执行 bash apps/client/scripts/build-lanpath-core.sh，生成未跟踪的 lanpathcore.aar。
进入 apps/client 执行 bun run tauri android dev --target aarch64，使用前端热更新。
编译 CLI 会重建被忽略的 tauri.settings.gradle 与 RustWebView JNI 胶水；不要提交本机生成路径。
不要跨检出共享 Android CARGO_TARGET_DIR；Tauri Kotlin 生成文件可能被旧构建缓存漏掉。
正式 APK 使用 bun run android:build。调试已有草稿的设备前先征得安装/重启许可。

## 工作流

`.github` 中的 YAML 文件统一使用 `.yaml` 扩展名。当前工作流为 `ci.yaml`、`windows-build.yaml`、`android-build.yaml` 与 `release.yaml`。

| 类型 | 使用时机 | 发现与计划 | 实现与验证 | 文档与最终复查 |
| --- | --- | --- | --- | --- |
| feature | 新用户能力 | 读取领域、平台边界与 ADR，列验收 | 最小完整实现，相关 verify/gates 和行为验收 | CHANGELOG、架构/API 文档，复查范围 |
| bugfix | 既有行为错误 | 复现、定位根因，列回归路径 | 最小修复、真实可用的回归检查 | 记录根因与验证边界，复查无旁支变更 |
| refactor | 结构改变 | 确认基线、行为不变目标 | 分步重构、相关 gates | 更新路径/架构，核对输出语义 |
| docs | 说明变更 | 核对实现和命令证据 | 修改并验证链接/命令 | 消除冲突，复查权威与措辞 |
| adoption | 协议采纳 | 读 ADOPT，Pass 1 inventory/mapping/audit | 无决策冲突后 Pass 2 生成、协调、验证 | schema/conformance/项目检查与 report |

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

设置 → 备份与恢复，设置至少 12 字符密码或生成 24 位随机密码，单独保存密码后导出。
文件位于 Downloads/Lanpath；Android 经 MediaStore 写入公共下载目录。导入 .zip.age 后输入
密码查看摘要，确认完整替换；代理停止后重启生效。Android 点击关闭应用按钮后，从桌面重新打开。
恢复前加密副本使用同一导入密码，可通过「使用恢复前备份」再次导入。核心、证书/私钥、
日志、流量历史、系统授权不迁移；跨设备的外部文件引用需检查，系统代理/TUN/MITM 手动重开。
新格式和中断恢复见 ADR-0008，测试记录见 testing/2026-10-17-encrypted-backup.md。

Android 启动先授权再准备配置，跨页 busy 由共享 mutation key 读取；备份预览五分钟有效、确认一次消费。
密码派生热循环在根与独立壳的 dev/release 均启用 opt-level=3，不降低加密参数，见 ADR-0009。
