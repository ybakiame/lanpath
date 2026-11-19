//! 测试专用：把 `tests/fixtures/core_fixture.rs` 用 `rustc` 编译成真实的
//! 假核心二进制（真实进程身份、真实监听、真实退出码），供运行时/校验器
//! 测试走完整的 spawn → pid 身份 → 监听 → 回收链路。与
//! `crates/lanpath-client/src/state/tests/helpers.rs` 的 lifecycle fixture
//! 同一模式：链接 Cargo 已构建的 `serde_json`/`serde_yaml` rlib，不引入
//! 新依赖。编译产物在临时目录，随测试进程退出清理。

use std::path::{Path, PathBuf};
#[path = "../tests/support/core_build.rs"]
mod core_build;

pub(crate) fn compiled_core() -> &'static Path {
    core_build::compiled_core_fixture()
}

/// 把假核心复制为库存布局 `cores/<binary>/<version>/<文件名>`（共享 on-disk
/// 命名规则），返回二进制路径。
pub(crate) fn seed_inventory_core(
    root: &Path,
    core_type: lanpath_common::CoreType,
    version: &str,
) -> PathBuf {
    copy_inventory_core(root, core_type, version)
}

/// 同 [`seed_inventory_core`]，并写行为 sidecar（`<binary>.behavior`），
/// 让假核心在调用方自合成配置时也能进入失败/早退/不监听等分支。
pub(crate) fn seed_inventory_core_with_behavior(
    root: &Path,
    core_type: lanpath_common::CoreType,
    version: &str,
    behavior: &serde_json::Value,
) -> PathBuf {
    let path = copy_inventory_core(root, core_type, version);
    std::fs::write(
        path.with_extension("behavior"),
        serde_json::to_vec(behavior).expect("serialize behavior"),
    )
    .expect("write behavior sidecar");
    path
}

fn copy_inventory_core(root: &Path, core_type: lanpath_common::CoreType, version: &str) -> PathBuf {
    let name = if cfg!(windows) {
        format!("{}.exe", core_type.binary_name())
    } else {
        core_type.binary_name().to_string()
    };
    let path = root
        .join("cores")
        .join(core_type.binary_name())
        .join(version)
        .join(name);
    std::fs::create_dir_all(path.parent().expect("inventory version directory"))
        .expect("create inventory directory");
    std::fs::copy(compiled_core(), &path).expect("copy fixture into inventory");
    path
}
