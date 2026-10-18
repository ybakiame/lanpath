# 规则集源与更新检测调研（2026-10-18）

## 现状与根因

市场使用本地静态 MetaCubeX 索引，添加 sing 分支 geo/geoip 或 geo/geosite 的 GitHub Raw SRS 地址。
此前更新通过 HEAD 的 Last-Modified 与本地下载时间比较；Raw 无该头时远端时间为 0，且每次重复下载。
HTTP 修改时间、本地文件写入时间和 Git 内容版本不是同一概念，不能互相代替。

对公开 `geo/geosite/google.srs` 的当次 HEAD 实测：

| 传输地址 | Last-Modified | ETag | 当次缓存头 |
| --- | --- | --- | --- |
| raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/... | 未提供 | 强 ETag | max-age=300 |
| cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/... | 未提供 | 弱 ETag | max-age=604800, s-maxage=43200 |
| testingcf.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/... | 未提供 | 弱 ETag | 同类 CDN 缓存头 |

这些是一次响应观察，不能当作长期可用性或缓存时长承诺。Raw 与 CDN 的 ETag 不同，不能跨地址复用。
单纯改成 jsDelivr 不会补出缺失的上游时间，分支 CDN 还可能晚于源站同步。

## 选择

继续使用 MetaCubeX GitHub 仓库作为内容来源，市场默认 GitHub Raw 下载，新增时可显式选 jsDelivr CDN。
已有条目地址保持不变；自定义 URL 仍受全局网络请求设置控制，GitHub 镜像前缀只用于 GitHub 请求。
明确区分用户分类（影音、工作等）与市场内容类型（GeoIP、GeoSite），不根据 URL 自动覆盖用户分类。

| 方案 | 优点 | 约束 | 本轮决定 |
| --- | --- | --- | --- |
| GitHub Raw | 直接跟踪上游分支，保留已有地址及镜像设置 | 可达性依赖用户网络；通常无修改时间 | 默认下载方式 |
| jsDelivr 分支 CDN | 可作为不同网络下的手动下载选择 | 分支缓存可能延迟；没有补足时间头 | 新增条目时可选 |
| GitHub commits API 按 path 查询 | 可获得文件提交记录和提交时间 | 未认证 REST 配额通常 60 次/小时；逐条查询不适合大量规则集；提交时间也不等于 CDN 当前内容时间 | 不加入默认更新路径 |
| 自建规则镜像及索引服务 | 可发布统一清单、内容散列、时间及来源 | 新增托管、构建和维护边界 | 本轮不引入 |

更新使用绑定实际请求 URL 和本地 SHA-256 的 HTTP 校验器：优先 If-None-Match，其次 If-Modified-Since。
304 且缓存可验证时跳过；200 比较文件字节，无变化不重写；无头时也可正确比较内容。
缺文件、内容不匹配或地址/镜像改变时不带旧条件，完整请求。失败、空响应保留旧缓存及成功检查信息。
SHA-256 只用于绑定缓存，不替代上游签名或来源认证。

页面展示本地写入时间、最近成功检查时间、源站提供的修改时间。未提供时间时显示“上游未提供”，
不使用 HTTP Date、下载时间或仓库整体提交时间填充。市场索引仍本地检索，不后台逐条请求 GitHub API。

## 一手资料

- [MetaCubeX/meta-rules-dat](https://github.com/MetaCubeX/meta-rules-dat)：上游仓库与各分支格式。
- [jsDelivr 使用及缓存说明](https://github.com/jsdelivr/jsdelivr)：GitHub/CDN 地址与缓存机制。
- [GitHub REST 配额](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api)：未认证 API 请求限制。
- [GitHub commits API](https://docs.github.com/en/rest/commits/commits#list-commits)：按分支、path 查询提交。

用户已选择上述推荐方案。实施及迁移见 ADR-0011。
