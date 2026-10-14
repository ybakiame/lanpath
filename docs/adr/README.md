# 架构决策

采用 Accepted / Proposed / Superseded 状态，记录日期、背景、决定及后果。

- [0001 独立仓库与正式身份](0001-independent-client.md) — Accepted
- [0002 桌面产品数据目录与便携分发](0002-desktop-portable-storage.md) — Accepted
- [0003 首次公开发版与签名身份](0003-first-public-release.md) — Accepted
- [0004 主开发环境迁至稳定 Nixpkgs](0004-stable-nixpkgs.md) — Accepted

源仓相关历史 ADR：0007 单壳双目标、0008 Windows 打包、0012 核心生命周期、0013 自研 UI、0014 窗口与策略。完整历史保留在来源仓库，不将旧服务端路径当作新仓构建契约。

## 继承决策的历史编号

注释中的 ADR-0002 至 ADR-0014 是源仓历史编号，本仓契约以架构文档和 ADR-0001 为准。

- [0004-client-singbox-only](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0004-client-singbox-only.md)
- [0007-client-single-shell-merge](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0007-client-single-shell-merge.md)
- [0012-client-core-lifecycle](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0012-client-core-lifecycle.md)
- [0003-desktop-mobile-split](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0003-desktop-mobile-split.md)
- [0008-windows-desktop-packaging](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0008-windows-desktop-packaging.md)
- [0014-client-window-and-proxy-ui](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0014-client-window-and-proxy-ui.md)
- [0005-client-config-slices](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0005-client-config-slices.md)
- [0011-shared-view-model-layer](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0011-shared-view-model-layer.md)
- [0013-client-own-ui-library](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0013-client-own-ui-library.md)
- [0006-client-traffic-stats](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0006-client-traffic-stats.md)
- [0002-client-rule-management](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0002-client-rule-management.md)
- [0010-desktop-bundled-core](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0010-desktop-bundled-core.md)
- [0009-client-repo-boundary](https://github.com/ybakiame/multi-proxy-panel/blob/cd09a8aec70886a8af17514a0b8d437b626c7822/docs/adr/0009-client-repo-boundary.md)
