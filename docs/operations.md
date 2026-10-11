# 运维、迁移与发布

新 Android 包名 com.ybakiame.lanpath 是独立应用，无法覆盖旧 com.proxypanel.client。
保留旧包；导入/重新配置后再验收。当前不提供跨包私有数据自动迁移，VPN/通知等权限需重新授权。
桌面默认 ~/.lanpath；新目录不存在而 ~/.proxy-panel-client 存在时沿用旧目录，不删除或复制旧数据。
不要同时运行新旧客户端共享同一个数据目录。旧 MITM CA 名称和系统信任关系保留，避免更名造成重建。
已有配置键和数据库 schema 保留，任何后续迁移使用版本化逻辑。

新仓 GitHub 地址预定为 ybakiame/lanpath。更新与项目链接采用此地址，但远端和首次 Release
准备好前不发布带新端点的正式包；开发阶段更新检查可能返回未发布错误。
updater 公钥保留，签名私钥只在新仓 Secrets 配置；Android 签名同样不入库。
首次身份更改无法作为旧 Android 包的原地升级；Windows 安装器更名需真实升级验收。

Windows desktop-test 手动构建 NSIS，仅上传 artifact；发布工作流通过手动输入匹配版本 tag，
构建包、生成签名更新清单后创建 Release。不同时更换打包方案。
Windows CI 例外使用锁定 Rust/Bun 和固定 Windows runner 标签；本地、Linux/Android 构建仍 Nix。
MSVC 14.44.35207 与 SDK 10.0.26100.0 显式选取，缺失时构建失败，不自动回退；由 runner 提供并记录版本，升级需单独复查；不声称托管镜像是不可变环境。
