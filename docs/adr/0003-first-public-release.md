# ADR-0003 首次公开发版与签名身份

状态：Accepted · 日期：2026-10-11

用户授权创建公开仓库 ybakiame/lanpath、使用 main、首次发版 v0.1.1-beta.1，并生成独立发布密钥。

## 决定

- main 为默认分支，main/dev 推送及 PR 执行 CI；版本使用 SemVer，beta 标记 GitHub prerelease，不推进稳定 latest。
- 新独立客户端采用专用 updater 验签公钥与 Android release 证书。密钥只保留在本机受限目录和 GitHub Secrets，替换继承公钥不迁移旧产品安装或数据。
- Windows 分发 x64/arm64 NSIS、签名 updater ZIP 与 Portable；延续已锁定的 native MSVC/Rust/Bun CI 例外。
- Android arm64 经项目 Nix 编译 Go AAR、Rust/Kotlin/资源，再用专用 keystore 签名 APK并验证；PR/debug CI 不接触发布密钥。
- 发布前要求同一 commit 的 CI 成功、两种架构 updater 签名验证成功、Android 签名 APK 构建成功，再生成摘要并发布 Release。
- 稳定版更新清单为 latest.json；测试版为 prerelease.json，测试版应用通过发布列表手动更新，Portable 仍保留 data 手动更新。
- 公开构建不读取私有协议子模块。普通开发的协议维护仍按既有 AGENTS 与采纳记录，本次不修改协议子模块。

## 后果

首发签名身份需要完整备份；丢失密钥会破坏后续升级连续性。
Windows 尚无 Authenticode 签名，updater 签名仅保证更新来源；beta 不能当作全部设备已验收的稳定版。
CI 编译与签名验证不代替真机 VPN/TUN 和安装升级验证。

依据：[Tauri updater 签名与打包](https://v2.tauri.app/plugin/updater/)、[Android release 签名](https://v2.tauri.app/distribute/sign/android/)。
