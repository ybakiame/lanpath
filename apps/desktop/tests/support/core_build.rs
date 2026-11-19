//! Cargo 测试核心构建适配：默认使用 deps；拆分缓存可显式指定依赖目录。
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

pub fn compiled_core_fixture() -> &'static Path {
    static COMPILED: LazyLock<(tempfile::TempDir, PathBuf)> = LazyLock::new(|| {
        let directory = tempfile::tempdir().expect("fixture tempdir");
        let executable = std::env::current_exe().expect("current test binary");
        let dependencies = std::env::var_os("LANPATH_TEST_DEPS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| executable.parent().expect("deps directory").to_path_buf());
        let mut candidates = std::fs::read_dir(&dependencies)
            .expect("read test dependency directory")
            .map(|entry| entry.expect("dependency entry").path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("libserde_json-") && name.ends_with(".rlib")
                    })
            })
            .collect::<Vec<_>>();
        // 共享缓存可能有不同 rustc 的产物；先试最近构建项，绝不清理宿主缓存。
        candidates
            .sort_by_key(|path| std::cmp::Reverse(path.metadata().and_then(|m| m.modified()).ok()));
        assert!(
            !candidates.is_empty(),
            "Cargo-built serde_json library missing; split caches can set LANPATH_TEST_DEPS_DIR"
        );
        let binary = directory.path().join(if cfg!(windows) {
            "core-fixture.exe"
        } else {
            "core-fixture"
        });
        let mut errors = String::new();
        for candidate in candidates {
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
                .arg(format!("serde_json={}", candidate.display()))
                .arg("-L")
                .arg(format!("dependency={}", dependencies.display()))
                .arg("-o")
                .arg(&binary)
                .output()
                .expect("run rustc for core fixture");
            if output.status.success() {
                return (directory, binary);
            }
            errors.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        panic!("core fixture compile failed: {errors}");
    });
    &COMPILED.1
}
