# 首次发版验证（2026-10-11）

目标：公开 ybakiame/lanpath，main，v0.1.1-beta.1；Windows x64/arm64 安装器与 Portable，Android arm64 release APK。

## 本地准备

用户确认公开仓库及生成 Lanpath 专用 updater / Android release 密钥。
私钥与密码仅在受限本机目录和 GitHub Secrets；提交公钥，未提交私钥、keystore 或密码。
协议子模块私有，不参与公开 CI 检出；云端构建不依赖协议内容。

本地检查均经项目 nix develop：

- 应用、client-core、UI 三包 verify 通过。
- cargo build --workspace 与 verify:rust 通过，包括独立壳 host clippy、Android target check。
- 5 个发布工具测试通过：x64/arm64 Portable 内容隔离、稳定/测试更新清单、Ed/ED 签名与篡改拒绝。
- 使用真实 Tauri signer 对临时 fixture 签名，并以提交公钥验证，检查格式与发布验证器兼容性。
- 工作流 YAML 解析与 shell 语法检查通过。

## 云端与设备边界

首次推送后运行 Client CI；同一提交 CI 成功后才允许 Client Release 分发。
云端运行结果、Release 链接与校验结果在完成后追加到本记录。
未进行 Windows/Android 真机安装、VPN/TUN 和真实升级验证；beta 首发不宣称这些设备行为已验收。
