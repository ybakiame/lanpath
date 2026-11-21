//! Cargo 测试核心构建适配：支持标准 `target/<profile>/deps` 与拆分缓存布局（如 `huacnlee/cargo`
//! 的 `shared-deps/<profile>/build/<crate>/<hash>/out`），也可通过 `LANPATH_TEST_DEPS_DIR` 覆盖。
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

const SERDE_JSON_DEP_CRATES: &[&str] = &[
    "serde_json",
    "serde",
    "serde_core",
    "itoa",
    "ryu",
    "memchr",
    "zmij",
];

fn find_rlibs_in_dir(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".rlib"))
        })
        .collect()
}

fn collect_split_cache_deps(executable: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let profile = if executable.components().any(|c| c.as_os_str() == "release") {
        "release"
    } else {
        "debug"
    };
    let Some(shared_build) = executable
        .ancestors()
        .map(|ancestor| ancestor.join("shared-deps").join(profile).join("build"))
        .find(|candidate| candidate.is_dir())
    else {
        return (Vec::new(), Vec::new());
    };
    let mut candidates = Vec::new();
    let mut search_dirs = Vec::new();
    for crate_name in SERDE_JSON_DEP_CRATES {
        let crate_dir = shared_build.join(crate_name);
        let Ok(hashes) = std::fs::read_dir(&crate_dir) else {
            continue;
        };
        for hash_entry in hashes.flatten() {
            let out_dir = hash_entry.path().join("out");
            if !out_dir.is_dir() {
                continue;
            }
            if *crate_name == "serde_json" {
                candidates.extend(find_rlibs_in_dir(&out_dir, "libserde_json-"));
            }
            search_dirs.push(out_dir);
        }
    }
    (candidates, search_dirs)
}

pub fn compiled_core_fixture() -> &'static Path {
    static COMPILED: LazyLock<(tempfile::TempDir, PathBuf)> = LazyLock::new(|| {
        let directory = tempfile::tempdir().expect("fixture tempdir");
        let executable = std::env::current_exe().expect("current test binary");
        let dependencies = std::env::var_os("LANPATH_TEST_DEPS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| executable.parent().expect("deps directory").to_path_buf());
        let mut candidates = find_rlibs_in_dir(&dependencies, "libserde_json-");
        let mut search_dirs = vec![dependencies];
        if candidates.is_empty() {
            let (split_candidates, split_dirs) = collect_split_cache_deps(&executable);
            candidates.extend(split_candidates);
            search_dirs.extend(split_dirs);
        }
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
            let mut cmd = std::process::Command::new("rustc");
            cmd.args([
                "--edition=2024",
                "--crate-name=lanpath_desktop_core_fixture",
            ])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/core_fixture.rs"
            ))
            .arg("--extern")
            .arg(format!("serde_json={}", candidate.display()));
            for dir in &search_dirs {
                cmd.arg("-L").arg(format!("dependency={}", dir.display()));
            }
            let output = cmd
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
