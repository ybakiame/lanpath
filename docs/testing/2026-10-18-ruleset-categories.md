# 规则集分类与更新回归（2026-10-18）

## 交付范围

- 用户单分类、未分类兼容、市场及编辑选择已有/新建分类。
- 管理列表按名称/tag 检索、分类筛选、多选整理；分类写入不改缓存与规则引用。
- 路由/DNS 共用筛选多选器，保留隐藏的已选项和失效引用；名称/tag 检索不区分大小写。
- GitHub Raw/CDN 新增时可选，已有地址不变；同一市场资源跨两种下载地址去重。
- 条件 GET/内容比较、独立的写入/检查/响应修改时间；缺失修改时间显示“上游未提供”。

## 环境与自动检查

所有项目命令在本仓 `nix develop --no-pure-eval` 执行。

- 根 workspace build、fmt/clippy/test；主库 557 项，Tauri 命令层 56 项通过。
- 独立桌面壳 host clippy、Android aarch64 target check。
- lanpath-app（桌面与 Android 构建）、client-core、ui 的 verify。
- lanpath-test-tools 10 项原有缓存/Portable/发布工具检查通过。
- Android debug APK/AAB、Kotlin 与资源构建；本轮无 Go 代码改动。

主要网络回归使用本机 Axum HTTP 服务，覆盖：ETag→304、缺缓存强制 GET、非空损坏缓存强制请求、
地址不匹配时不复用校验器、无头时比较内容、空响应/HTTP 错误保留原件与成功元数据。
已有 Last-Modified 测试更新为 GET 响应提供头，不再以本地时钟比较断言“已最新”。

迁移测试覆盖：原始字节副本、版本幂等、导入不同旧快照时分别保留原件、副本创建失败不改源配置。
命令测试覆盖：归类不重写文件、输入的校验器/远程写入时间不可伪造、换源重置校验器、
慢更新回写保留期间编辑的分类、不按设备时钟排序结果、不复活已删除条目。

## 浏览器交互（mock）

使用 agent-browser 独立会话、Vite 的 Android 与 desktop 编译模式，全部数据为 example.invalid 演示项：

1. 名称检索 Netflix 只显示目标项；批量整理新建“娱乐”后，返回的视图更新且仅调用分类命令。
2. 市场选择“工作”与 jsDelivr，再添加 google，保存的 category 与 CDN 地址正确。
3. 路由规则选择 Netflix/GitHub，再组合搜索与分类使结果为空，已选两项仍在；保存后 tag 集合未改变。
4. DNS 规则检索 NETFLIX，仍保留两个已选项，保存后 DNS target 不变。
5. 移动 390×844 浅色、375×812 深色及桌面 1280×900、760×560 无横向溢出。
6. 搜索框移动高度 48px、桌面 40px；分类弹层可选择、新建、返回上层；原值保留且按钮有可访问名。

截图保留本机临时目录，不提交生成图片。使用 mock 仅验证 UI 和发出的命令形态。

## 真实源观测与设备边界

公开源 HEAD 观测见 docs/research/2026-10-18-ruleset-sources.md：Raw 与两个 CDN 都无 Last-Modified，均有 ETag。
未安装或重启用户设备；Windows 原生实际 HTTP、Android 真机读写及加密备份携带分类需设备复测。
不保证某下载域名在所有网络可达；代理设置仍由用户配置。无上游时间并不等于未更新或检查失败。
