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
已通过的云端检查：提交 `2f11f20` 的 [Client CI](https://github.com/ybakiame/lanpath/actions/runs/38129409384)，包括完整 Android debug APK；提交 `fbe19c3` 的 [Windows 双架构签名构建](https://github.com/ybakiame/lanpath/actions/runs/38128841494)，包括 NSIS、updater 与 Portable。下载两种架构的 updater ZIP 后，本地经项目 Nix 使用提交公钥验签通过。
旧提交 `fbe19c3` 的 CI 被新提交的 CI 自动取消（同一分支 concurrency、cancel-in-progress），并非 Release 失败；改名后提交的 CI 已成功。
首次 Release 的 Android APK 经本地项目 Nix 再次 apksigner 验证通过；证书 SHA-256 为 `DA:9B:CD:CB:68:1C:EE:0B:48:F1:3F:64:9A:11:92:31:C6:2A:BD:E0:37:7F:E6:E1:73:E4:CB:EC:EA:27:4E:11`，与生成的专用身份一致。aapt2 确认包名 `com.ybakiame.lanpath`、版本 `0.1.1-beta.1`、minSdk 33、targetSdk 36；APK 仅含 arm64-v8a 原生库。
提交 `2f11f20` 的 [Client Release](https://github.com/ybakiame/lanpath/actions/runs/38130449205) 全部 job 成功，已公开发布 [v0.1.1-beta.1](https://github.com/ybakiame/lanpath/releases/tag/v0.1.1-beta.1)，标记 prerelease。发布包含 13 个文件（12 个产物及 SHA256SUMS）。
下载全部公开文件后，12 个摘要均匹配；两个 updater ZIP 使用提交公钥验签通过；prerelease.json 的版本、双架构 URL 与签名均匹配发布文件，没有 latest.json。两种 Portable 的程序与种子核心 PE 架构分别为 x64/arm64，包含便携标记和空 data/，顶层文件符合打包白名单。公开 APK 与已验签的构建 artifact 摘要一致。
未进行 Windows/Android 真机安装、VPN/TUN 和真实升级验证；beta 首发不宣称这些设备行为已验收。
