# 独立仓库验证记录

环境：本仓 Nix，Bun 1.4.2、Rust 1.98.0、Go 1.26.7、JDK 17、NDK 28.0.13004108。

- 三个前端包 verify：通过，包含桌面/Android 前端、类型、lint 和格式检查。
- cargo build --workspace：通过。完整 Rust 门禁：通过，包含根 fmt/clippy/test、桌面壳 host clippy 和 Android target check，无目标检查跳过。
- Rust：664 项测试通过；3 项显式 opt-in 测试 ignored，真实 sing-box E2E 未执行。
- ZIP 回归：Stored/Deflate 两种 Windows 核心压缩包及嵌套 exe 提取通过。
- 新品牌 Go AAR：通过，Java 类包 com.ybakiame.lanpath.core.libbox。
- 完整 Android debug：通过，APK/AAB；APK 身份 com.ybakiame.lanpath，版本 0.1.1，arm64-v8a。未安装用户设备。
- 干净暂存源码快照：冻结依赖安装、锁定 Cargo 元数据、AAR 重建及独立 target 下 Android APK/AAB 通过，不含旧仓源码或本机生成文件。
- 协议三份 schema、13 项能力映射、文档链接、工作流 YAML、干净固定子模块和发布清单成功/拒绝路径：通过。
- Windows CI 和真机验收：未执行。新远端/签名/正式发布：未执行。

初次独立测试发现 Nix 缺少压缩运行库，已补齐；Android ZIP 默认功能误链接主机库，改为核心下载所需 Deflate。
跨目录共享 Android target 会漏重建 TauriActivity 等生成代码；干净验证改为独立 target，不能用原目录生成文件证明可独立构建。
来源及子模块修订见 source-provenance.json。原 ProxyPanel 仓库和数据保留，未移动或删除。
