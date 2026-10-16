# ADR-0007：订阅请求的代理与会话兼容

- 状态：Accepted
- 日期：2026-10-16
- 触发：对照本地 GUI.for.SingBox 参考实现，排查 Windows/Android 下载失败及需要连续请求的订阅。

## 决策

订阅、客户端远程资源、核心版本查询与下载共用请求代理策略，与 GitHub 镜像前缀解耦。
设置 → 网络请求提供 `request_proxy_mode`：`none`（明确直连）、`system`（系统/环境代理）、
`core`（本地 mixed 端口）和 `custom`（`request_proxy_url`，支持 HTTP(S)/SOCKS5）。
显式代理失败不回退直连；保存对下一次请求生效，核心大文件下载不增加总超时。
Windows/macOS 系统发现包含操作系统设置与环境变量，Linux 主要使用环境变量；无配置直连。
Android 系统 VPN 不等同于进程 HTTP 代理，当前系统发现不读取 Java ConnectivityManager：
应用自身被 VPN 排除时，可选择核心代理，需核心已运行。
GitHub 前缀仅改写 GitHub URL，不决定连接使用的代理，脚本执行器的请求策略不在本次范围内。

用户已明确同意推荐迁移方案。首装默认 `system`；已保存配置缺少新字段时一次性 v1 升级，
旧 `fetch_via_local_proxy=true` → `core`，false/缺失 → `none`，移除旧字段。
升级前将原始文件原样保存为 `client.request-proxy-v1.backup.json`，不覆盖已有备份，
校验配置后用临时文件原子替换；失败返回错误、保留原文件，可修复后重试。
新字段存在表示完成，不在读写热路径双写旧字段。回退可退出客户端后用备份恢复 client.json。

为每次订阅拉取创建临时 cookie jar。首次收到设置 cookie 的 HTML 页面时立即重试一次；
连接失败/请求超时也最多重试一次。普通 401 等错误不无条件重试，不执行 HTML/JavaScript，
反复 HTML 挑战报明确错误。会话 cookie 不写入业务缓存或配置。

启用 gzip、Brotli、deflate HTTP 解压；解压后的订阅文本仍保留原始格式、注释与换行，
仅有效订阅响应写入原文缓存，不把 HTML 挑战当作原文。默认 UA 保持 clash.meta，
用户自定义 UA 继续生效。证书校验保持开启，错误展示连接/TLS 根因并移除请求 URL。

reqwest 最低约束对齐已有锁定版本 0.12.28，以显式启用该版本的 system-proxy；
继续禁用 native-tls/default-tls，不给 Android 引入 OpenSSL 依赖。

## 参考对比与边界

参考程序 frontend/src/stores/subscribes.ts 支持请求方式、请求头、超时与代理选项；
bridge/utils.go 复用 Go 默认 transport（gzip），frontend 的 GetRequestProxy 可查询系统代理。
Go HTTP client 本身未设置 CookieJar；本次针对实际观测的 cookie 落地页增加了有界会话重试，
不声称参考程序已有同样的自动重试。

参考程序另有跳过证书验证、每订阅自定义代理/任意请求头等选项，本次不复制这些配置接口。
其默认 UA clash.meta/mihomo 与本仓不同，但实测当前 UA 能处理用户提供的两个订阅，故不改默认值。

官方行为说明：https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html
