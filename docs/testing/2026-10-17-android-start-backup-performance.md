# Android 授权与备份性能回归

## 发现

用户已确认此前 Windows 导出→Android 导入成功；约 25 KiB 备份恢复体感两分钟，首次启动迟到授权且切页可重复触发。
代码确认预览/确认重复解密，加上回滚加密共三次 KDF；Android debug 未优化热循环，release 使用 z。
移动首页先完整 start_proxy，遇未授权才发请求；busy 属于局部 observer，重挂载丢失。

## 修正与本地证据

- 保持 age/scrypt N=2^17、格式 v1，根和独立壳的 scrypt/salsa20 在 dev/release 采用 opt-level=3。
- 一次性随机凭证复用预览快照；5 分钟到期，定时清理、消费与替换均释放 SecretString。
- 凭证错误/过期在停止核心前拒绝，已校验数据不可被确认时的另一个文件替换；加密回滚副本保留。
- Android 授权移到配置/下载前；全局启动门禁 try_lock 拒绝重复请求；Kotlin AtomicBoolean 守护实际弹窗。
- 前端启动 mutation key 跨页共享；去授权只在用户点击时重试，不自动在取消后弹第二次。
- 备份定向组 10 项主机 debug 测试通过，约 1.83 秒；此前 6 项约 38 秒。不同测试组但包含原加密往返与篡改场景，主机结果不等于手机耗时。
- Query MutationObserver 回归覆盖离页取消订阅后全局 pending 仍为 1、新 observer 本地 pending 为 false，完成后全局归零。

## 发布验证与边界

执行三个前端包 verify、workspace build + 完整 Rust fmt/clippy/test、独立壳 host clippy 与 Android check、
工具回归及 Android Kotlin/资源和 APK/AAB 构建；最终完成结果在下文补记。
浏览器 mock 验证启动挂起时切页/返回仍禁用，取消后不自动重试；不能证明真实系统授权弹窗或手机 KDF 时间。
发布 v0.1.3-beta.1 沿用已有 Windows updater/Android 签名身份及 beta 通道策略，无备份格式迁移。

本地完整 Rust 门禁通过：客户端 547 项、其余 workspace 测试、桌面壳 host clippy 和 Android check。
三个前端包 verify、10 项工具回归通过。浏览器 mock 确认跨页返回按钮仍禁用、请求保持一次；
拒绝后显示手动授权入口且不自动重试，显式重试后成功进入运行状态。未操作用户真实客户端。
Android debug APK 与 AAB 最终完整构建成功，包含新的 Kotlin 原子授权门禁与 0.1.3-beta.1 元数据。
