# ADR-0001: 独立客户端仓库与正式身份

Status: Accepted。Date: 2026-10-11。Decider: 用户本轮明确授权。

## 背景

ProxyPanel 客户端已确立岚径 / Lanpath 品牌，需要独立开发、构建和版本。

## 决定

两产品平级仓库；Lanpath 不作为 ProxyPanel 子模块，只引入 Agent Development Protocol 子模块。
桌面/Android 单壳双目标和共享 UI 保留，运行核心仅 sing-box。
基础错误与运行管理收窄为客户端自有模块，Go 模块/AAR 品牌化为 lanpathcore。
应用身份 com.ybakiame.lanpath，产品名 Lanpath；初始快照保留源仓许可/归属及来源记录。
本地构建运行用本仓 Nix。用户明确授权 Windows CI 使用锁定版本原生工具链，
Linux/Android CI 用 Nix；MSVC SDK 托管镜像变化记录为限制。

## 后果

Android 改包名是新应用；不会读取旧包私有数据。桌面优先已有兼容目录，不自动删旧数据。
已有 UI 存储键、数据字段和 CA 主体保留。新仓签名、发布地址及 Windows 升级仍需部署验收。
原仓未删除客户端，在独立新仓验证完成后另行清理；不迁移原仓本身。
