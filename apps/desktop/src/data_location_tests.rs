use super::*;
use std::{fs, process::Command};

fn executable_in(directory: &Path) -> PathBuf {
    directory.join(format!(
        "lanpath-desktop-nsh{}",
        std::env::consts::EXE_SUFFIX
    ))
}

#[test]
fn portable_location_is_beside_executable_and_explicit_override_wins() {
    let directory = tempfile::tempdir().unwrap();
    let executable = executable_in(directory.path());
    fs::write(directory.path().join(PORTABLE_MARKER), b"").unwrap();
    assert_eq!(
        DataLocation::resolve(&executable, None).unwrap().path,
        directory.path().join("data")
    );
    let explicit = directory.path().join("test-override");
    assert_eq!(
        DataLocation::resolve(&executable, Some(&explicit))
            .unwrap()
            .path,
        explicit
    );
    assert!(
        !directory.path().join("data").exists(),
        "resolution alone never writes"
    );
}

#[test]
fn dot_portable_is_ignored_and_standard_location_is_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join(".portable"), b"").unwrap();
    assert_eq!(
        DataLocation::resolve(&executable_in(directory.path()), None)
            .unwrap()
            .path,
        default_data_dir().unwrap()
    );
}

#[test]
fn invalid_marker_never_falls_back_or_creates_data() {
    for is_directory in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join(PORTABLE_MARKER);
        if is_directory {
            fs::create_dir(&marker).unwrap();
        } else {
            fs::write(&marker, b"not empty").unwrap();
        }
        assert!(DataLocation::resolve(&executable_in(directory.path()), None).is_err());
        assert!(!directory.path().join("data").exists());
    }
}

#[test]
fn copied_native_process_uses_own_portable_root_and_preserves_it_after_move() {
    let directory = tempfile::tempdir().unwrap();
    let initial = directory.path().join("first");
    let moved = directory.path().join("moved");
    fs::create_dir(&initial).unwrap();
    fs::copy(std::env::current_exe().unwrap(), executable_in(&initial)).unwrap();
    fs::write(initial.join(PORTABLE_MARKER), b"").unwrap();
    let run = |program: &Path, override_dir: Option<&Path>| {
        let expected = override_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| program.join("data"));
        let mut command = Command::new(executable_in(program));
        command
            .args([
                "--exact",
                "data_root::location::tests::portable_child",
                "--ignored",
                "--nocapture",
            ])
            .current_dir(directory.path())
            .env_remove(DATA_DIR_ENV)
            .env("LANPATH_PORTABLE_EXPECT", &expected);
        if let Some(path) = override_dir {
            command.env(DATA_DIR_ENV, path);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&initial, None);
    let identity = fs::read(initial.join("data/.lanpath-desktop-nsh-root-id")).unwrap();
    let marker = fs::read(initial.join("data/.lanpath-desktop-nsh")).unwrap();
    let rules = fs::read(initial.join("data/rules.toml")).unwrap();
    fs::rename(&initial, &moved).unwrap();
    run(&moved, None);
    assert_eq!(
        fs::read(moved.join("data/.lanpath-desktop-nsh")).unwrap(),
        marker
    );
    assert_eq!(
        fs::read(moved.join("data/.lanpath-desktop-nsh-root-id")).unwrap(),
        identity
    );
    assert_eq!(fs::read(moved.join("data/rules.toml")).unwrap(), rules);
    run(&moved, Some(&directory.path().join("explicit")));
    assert!(
        !directory.path().join("data").exists(),
        "working directory is irrelevant"
    );
}

#[test]
fn renamed_lanpath_exe_in_own_portable_directory_initializes_cleanly() {
    let directory = tempfile::tempdir().unwrap();
    let app_dir = directory.path().join("Lanpath");
    fs::create_dir(&app_dir).unwrap();
    let renamed = app_dir.join("Lanpath.exe");
    fs::copy(std::env::current_exe().unwrap(), &renamed).unwrap();
    fs::write(app_dir.join(PORTABLE_MARKER), b"").unwrap();

    let run = || {
        let output = Command::new(&renamed)
            .args([
                "--exact",
                "data_root::location::tests::portable_child",
                "--ignored",
                "--nocapture",
            ])
            .current_dir(directory.path())
            .env_remove(DATA_DIR_ENV)
            .env("LANPATH_PORTABLE_EXPECT", app_dir.join("data"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };
    // 首次创建 data/ 及二次打开都应顺利通过祖先目录检查。
    run();
    run();
    assert!(app_dir.join("data/runtime/db").is_dir());

    // 若同目录存在旧版 Tauri 遗留 seed/ 目录，仍必须拒绝初始化。
    fs::create_dir(app_dir.join("seed")).unwrap();
    let rejected = Command::new(&renamed)
        .args([
            "--exact",
            "data_root::location::tests::portable_child",
            "--ignored",
        ])
        .current_dir(directory.path())
        .env_remove(DATA_DIR_ENV)
        .env("LANPATH_PORTABLE_EXPECT", app_dir.join("data"))
        .output()
        .unwrap();
    assert!(
        !rejected.status.success(),
        "presence of legacy seed/ must still be rejected"
    );
}

#[test]
#[ignore = "invoked by copied_native_process with isolated executable and data directories"]
fn portable_child() {
    let expected = PathBuf::from(std::env::var_os("LANPATH_PORTABLE_EXPECT").unwrap());
    let root = prepare_native_data_dir().unwrap();
    assert_eq!(root, expected.canonicalize().unwrap());
    assert_eq!(resolve_data_location().unwrap().path, root);
    let service = crate::rules::Rules::for_root(&root).unwrap();
    if !service
        .load()
        .unwrap()
        .iter()
        .any(|rule| rule.target == "isolated.example.com")
    {
        service
            .add_lines("suffix isolated.example.com -> proxy", true)
            .unwrap();
    }
    assert_eq!(prepare_native_data_dir().unwrap(), root);
}
