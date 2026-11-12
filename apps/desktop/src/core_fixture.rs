//! 测试专用：把 `tests/fixtures/core_fixture.rs` 用 `rustc` 编译成真实的
//! 假核心二进制（真实进程身份、真实监听、真实退出码），供运行时/校验器
//! 测试走完整的 spawn → pid 身份 → 监听 → 回收链路。与
//! `crates/lanpath-client/src/state/tests/helpers.rs` 的 lifecycle fixture
//! 同一模式：链接 Cargo 已构建的 `serde_json`/`serde_yaml` rlib，不引入
//! 新依赖。编译产物在临时目录，随测试进程退出清理。

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// 编译后的假核心二进制路径（进程内只编译一次）。
pub(crate) fn compiled_core() -> &'static Path {
    static COMPILED: LazyLock<(tempfile::TempDir, PathBuf)> = LazyLock::new(|| {
        let directory = tempfile::tempdir().expect("fixture tempdir");
        let executable = std::env::current_exe().expect("current test binary");
        let dependencies = executable.parent().expect("deps directory");
        let rlib = |prefix: &str| {
            std::fs::read_dir(dependencies)
                .expect("read deps directory")
                .map(|entry| entry.expect("deps entry").path())
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".rlib"))
                })
                .unwrap_or_else(|| panic!("Cargo-built {prefix} library"))
        };
        let output = std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-name=lanpath_desktop_core_fixture",
            ])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/core_fixture.rs"
            ))
            .arg("--extern")
            .arg(format!("serde_json={}", rlib("libserde_json-").display()))
            .arg("-L")
            .arg(format!("dependency={}", dependencies.display()))
            .arg("-o")
            .arg(directory.path().join("core-fixture"))
            .output()
            .expect("run rustc for the core fixture");
        assert!(
            output.status.success(),
            "core fixture compile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let binary = directory.path().join("core-fixture");
        (directory, binary)
    });
    &COMPILED.1
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
