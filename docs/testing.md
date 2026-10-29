# 验证

先从仓库根进入 `devenv shell`，再运行以下检查；非交互形式为 `devenv shell -- COMMAND ...`。环境入口与唯一 devenv.lock 的迁移见 [ADR-0015](adr/0015-standalone-devenv.md)。这些是执行指引，不表示本次已经运行；docs/testing/ 下的历史记录仍保留当时实际入口和结果。

~~~bash
bun run --filter lanpath-app verify
bun run --filter @lanpath/client-core verify
bun run --filter @lanpath/ui verify
cargo build --workspace
bun run verify:rust
~~~

Android Go/JNI 改动先构建 lanpathcore.aar，再用 Tauri Android CLI 生成本机胶水，
可执行 Gradle wrapper 的 :app:compileArmDebugKotlin -x rustBuildArm64Debug 检查 Kotlin/资源。
交叉 Rust check 不代替 AAR/Kotlin 编译或 VPN 真机验收。
前端暂无单元测试运行器；需记录手工验证。真实核心集成测试默认 ignored，
使用 LANPATH_TEST_SINGBOX 并显式 --include-ignored 启用。

迁移验收：独立依赖、正式名称/包名、策略选中无 chip、日志筛选、IME、关于信息、
Windows 标题栏与 MITM 无闪窗、核心退出清理、Android VPN 与授权。
记录每项实际执行的平台，不能把 browser mock 当作真实设备结论。
