# devenv Flake 集成验证（2026-10-15）

## 配置与缓存

devenv v2.4.0 锁定 b904dcb51fe48c30db250038241507f60752f222；主 nixpkgs 仍为已验证的 26.05 快照，Android SDK 输入未更新。项目仅使用 flake.lock，未生成 devenv.lock。
工具、桌面、Android 和共用脚本分模块；全局 Gradle 已移除，实际 Android 构建使用项目 wrapper。Rust 仍为 rustup + rust-toolchain.toml 的 1.98.0，未叠加 devenv Rust 模块。
任务辅助程序已实际从 devenv.cachix.org 下载并由 Nix 签名机制验证；主缓存与 vendor 缓存在同一明确列表中，避免主列表覆盖额外缓存。CI 显式配置 vendor 公钥，不依赖本机代理。

## 实际验证

在 nix develop --no-pure-eval 中通过本地/CI 共用命令执行：

- lanpath-install：冻结 Bun 锁文件安装通过。
- lanpath-verify-frontend：app 双目标前端 build/lint/format、client-core、UI verify 通过。
- lanpath-test-tools：9 项回归通过。
- lanpath-verify-rust：根 workspace build/fmt/clippy/test、独立桌面壳 host clippy、Android target check 通过。
- lanpath-build-aar：以 GOTOOLCHAIN=local 构建 lanpathcore.aar 通过。
- lanpath-build-android-debug：Rust/Kotlin/资源编译通过，生成 debug APK/AAB，未安装设备。
- 从 apps/client 子目录进入同一 Flake 后调用共用脚本可回到 Git 根；模拟首条 Bun 检查退出 42，脚本立即返回 42，后续两项未执行。
- 进入 shell 仅执行 devenv 初始化；未自动安装项目依赖或构建业务产物。core.hooksPath 仍为 .husky/_。
- 工作流 YAML 解析、内部引用、.envrc shell 语法及 git diff --check 通过。

## 边界

未运行新配置的云端 CI、Windows 构建或真机验收；未使用正式签名身份构建 release APK，也未创建新 Release。没有测量 cold/warm CI 耗时，因此不宣称具体加速。工具模块版本保持 Bun 1.4.2、Go 1.26.8、Rust 1.98.0、JDK 17、NDK 28.0.13004108。
