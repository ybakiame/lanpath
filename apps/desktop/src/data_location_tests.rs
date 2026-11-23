use super::*;
use std::{fs, process::Command};

fn executable_in(directory: &Path) -> PathBuf {
    directory.join(format!("lanpath{}", std::env::consts::EXE_SUFFIX))
}

#[test]
fn writable_data_dir_beside_executable_enables_portable_mode_and_explicit_override_wins() {
    let directory = tempfile::tempdir().unwrap();
    let executable = executable_in(directory.path());
    let data = directory.path().join(PORTABLE_DIR_NAME);
    fs::create_dir(&data).unwrap();
    assert_eq!(DataLocation::resolve(&executable, None).unwrap().path, data);
    // 写权限探测完成后不得留下临时探测文件，保证 data/ 仍为空目录可正常初始化。
    assert!(fs::read_dir(&data).unwrap().next().is_none());
    let explicit = directory.path().join("test-override");
    assert_eq!(
        DataLocation::resolve(&executable, Some(&explicit))
            .unwrap()
            .path,
        explicit
    );
}

#[test]
fn missing_or_non_directory_data_uses_etcetera_standard_directory() {
    let directory = tempfile::tempdir().unwrap();
    let expected = default_data_dir().unwrap();
    assert!(expected.is_absolute());
    assert_eq!(
        expected.file_name().and_then(|name| name.to_str()),
        Some(APP_DIR_NAME)
    );
    assert_eq!(
        DataLocation::resolve(&executable_in(directory.path()), None)
            .unwrap()
            .path,
        expected
    );

    // 同级 `data` 为普通文件而非目录时，不触发便携模式。
    fs::write(directory.path().join(PORTABLE_DIR_NAME), b"not a dir").unwrap();
    assert_eq!(
        DataLocation::resolve(&executable_in(directory.path()), None)
            .unwrap()
            .path,
        expected
    );
}

#[cfg(unix)]
#[test]
fn readonly_or_symlink_data_dir_falls_back_to_standard_directory() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let expected = default_data_dir().unwrap();

    // 符号链接 data 不作为便携目录。
    let link_dir = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    symlink(target.path(), link_dir.path().join(PORTABLE_DIR_NAME)).unwrap();
    assert_eq!(
        DataLocation::resolve(&executable_in(link_dir.path()), None)
            .unwrap()
            .path,
        expected
    );

    // 不可写的 data 目录回退到平台标准目录（非 root 用户生效）。
    let ro_dir = tempfile::tempdir().unwrap();
    let ro_data = ro_dir.path().join(PORTABLE_DIR_NAME);
    fs::create_dir(&ro_data).unwrap();
    fs::set_permissions(&ro_data, fs::Permissions::from_mode(0o555)).unwrap();
    let resolved = DataLocation::resolve(&executable_in(ro_dir.path()), None)
        .unwrap()
        .path;
    fs::set_permissions(&ro_data, fs::Permissions::from_mode(0o755)).unwrap();
    if resolved != ro_data {
        assert_eq!(resolved, expected);
    }
}

#[test]
fn copied_native_process_uses_own_portable_root_and_preserves_it_after_move() {
    let directory = tempfile::tempdir().unwrap();
    let initial = directory.path().join("first");
    let moved = directory.path().join("moved");
    fs::create_dir(&initial).unwrap();
    fs::copy(std::env::current_exe().unwrap(), executable_in(&initial)).unwrap();
    fs::create_dir(initial.join(PORTABLE_DIR_NAME)).unwrap();
    let run = |program: &Path, override_dir: Option<&Path>| {
        let expected = override_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| program.join(PORTABLE_DIR_NAME));
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
    let identity = fs::read(initial.join("data/.lanpath-root-id")).unwrap();
    let marker = fs::read(initial.join("data/.lanpath-root")).unwrap();
    let rules = fs::read(initial.join("data/rules.toml")).unwrap();
    assert!(initial.join("data/runtime/db").is_dir());
    fs::rename(&initial, &moved).unwrap();
    run(&moved, None);
    assert_eq!(fs::read(moved.join("data/.lanpath-root")).unwrap(), marker);
    assert_eq!(
        fs::read(moved.join("data/.lanpath-root-id")).unwrap(),
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
