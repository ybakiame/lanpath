# 版本约定

Lanpath 使用 [SemVer 2.0.0](https://semver.org/lang/zh-CN/)。当前处于 `0.1.x-rc.N` 候选阶段；`VERSION` 是产品版本唯一来源，当前值见该文件。根共享 Rust crates 与 NSH 桌面应用使用同一版本。GPUI 实验应用继续使用独立应用版本，其锁文件中的共享 crates 版本同步更新；Android 原生库尚无 APK 版本。

当前公共契约包括共享 Rust API、持久化配置格式、桌面命令及用户可见行为。`0.x` 阶段这些契约尚未稳定；当前没有需要兼容的旧用户数据，不实现旧配置迁移或兼容分支。测试与验收从新数据根开始。未来产生正式用户数据后，变更格式前应另行决定兼容策略。

同一候选目标的下一次交付递增 `rc.N`，例如 `0.1.4-rc.1` → `0.1.4-rc.2`；切换下一修订目标使用 `0.1.5-rc.1`。候选目标达到发布条件后去掉后缀为 `0.1.4`，同时调整版本脚本中的阶段约束。`rc.2` 小于 `rc.10`，所有候选版均小于同一目标的正式版；不按字符串排序版本。进入稳定公共契约时再发布 `1.0.0`。

## 更新与检查

从仓库根运行（Linux 先进入 `devenv shell`）：

```sh
python3 scripts/version.py --set 0.1.4-rc.2
python3 scripts/version.py --check
python3 scripts/version.py --check --tag v0.1.4-rc.2
```

脚本只更新产品清单与 Cargo 锁文件中本仓包的版本，不解析/升级第三方依赖，不更改 `devenv.lock`。`apps/desktop/nsh.json` 的 `build` 为平台构建序号，独立于语义版本，只有平台分发需要时递增。

应用内版本、`--version` 和 Windows 文件详情来自编译时 Cargo 版本；Windows 字符串保留完整 `rc.N`，数值资源使用 `major.minor.patch.N`。构建时校验桌面 Cargo 版本与 `VERSION` 一致，清单与各锁文件通过上述检查校验。

## 交付记录

候选开发期间将用户可见行为写入 `CHANGELOG.md` 的 `Unreleased`。准备发布时移到带日期的版本节，验证根共享库及独立桌面目标、检查 Windows 原生交互后，再创建 `vVERSION` 标签。发布后的标签与制品不可覆盖；修复必须使用新候选号。当前改动不自动创建标签、不上传或发布制品。历史 CHANGELOG 条目保持原样，不代表本阶段提供旧数据兼容。
