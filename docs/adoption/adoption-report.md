# Lanpath 协议采纳报告

2026-10-11，协议版本 0.1.0。状态：partial；项目契约核验通过，Windows/发布验收待完成。

先记录导出代码事实和源仓修订，再执行 Pass 1 inventory/mapping/audit；之后生成 Lanpath 专属契约。
[审计](audit-pass1.md)、[能力映射](protocol-mapping.json) 与 [机器报告](adoption-report.json) 保留阶段证据。
六项必需能力使用本仓文档目录约定；七项 AI 运行时/厂商适配能力记录为不适用。
三份 schema、13 项能力覆盖、证据路径、文档链接、工作流 YAML 和固定协议修订均通过检查。
不修改协议内容，未引入新的强制门禁，不声称厂商适配器已完成一致性评测。

本仓 Nix 下前端三包、Rust 完整门禁、Go AAR 和 Android APK/AAB 均通过。
不含生成文件的暂存源码快照再次完成冻结依赖安装、AAR 和独立 target 下 Android 打包。
Windows CI 原生锁定工具链例外已明确批准，记录在 ADR-0001。
真实 Windows 构建/升级、新远端及发布 Secrets 尚未验收；Android 新包不迁移旧包私有数据。
