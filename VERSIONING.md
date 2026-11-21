# 版本约定

Lanpath 使用 [SemVer 2.0.0](https://semver.org/lang/zh-CN/)。当前处于 `0.2.x-rc.N` 候选阶段（`0.2.1-rc.1` 为首个 NSH 原生桌面候选版本）；`VERSION` 是产品版本唯一来源，根共享 Rust crates 与 `apps/desktop`（NSH 桌面应用）使用同一版本；Android 原生库尚无 APK 版本。

当前公共契约包括共享 Rust API、持久化配置格式、桌面命令及用户可见行为。`0.x` 阶段这些契约尚未稳定，不保留历史版本兼容分支或旧数据迁移路径，测试与验收均从新数据根开始。进入稳定公共契约时再发布 `1.0.0`。

同一候选目标的下一次交付递增 `rc.N`，例如 `0.2.1-rc.1` → `0.2.1-rc.2`；切换下一修订目标使用 `0.2.2-rc.1`。候选目标达到发布条件后去掉后缀为 `0.2.1`，同时调整版本脚本中的阶段约束。`rc.2` 小于 `rc.10`，所有候选版均小于同一目标的正式版；不按字符串排序版本。

## 更新与检查

从仓库根运行（Linux 先进入 `devenv shell`）：

```sh
python3 scripts/version.py --set 0.2.1-rc.1
python3 scripts/version.py --check
python3 scripts/version.py --check --tag v0.2.1-rc.1
```

脚本只更新产品清单与 Cargo 锁文件中本仓包的版本，不解析或升级第三方依赖，不更改 `devenv.lock`。`apps/desktop/nsh.json` 的 `build` 为平台构建序号，独立于语义版本，仅在平台分发需要时递增。

应用内版本、`--version` 和 Windows 文件详情来自编译时 Cargo 版本；Windows 字符串保留完整 `rc.N`，数值资源使用 `major.minor.patch.N`。构建时校验桌面 Cargo 版本与 `VERSION` 一致，清单与锁文件通过上述脚本检查。

## 交付记录

候选开发期间将面向用户的行为变更写入 `CHANGELOG.md` 的 `Unreleased`。准备发布时移至带日期的版本节，验证根共享库及桌面目标、检查 Windows 原生交互后再创建 `vVERSION` 标签。发布后的标签与制品不可覆盖，修复须递增候选号；常规提交不自动创建标签或发布制品。
