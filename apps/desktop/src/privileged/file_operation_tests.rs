use super::{Apartment, run, session};
use std::{fs, marker::PhantomData};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

#[test]
fn mta_callers_can_promote_binary_files_without_overwriting_existing_payloads() {
    std::thread::spawn(|| {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .unwrap();
        let _caller_apartment = Apartment(PhantomData);
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.bin");
        let payload = b"authenticated binary\0\xff";
        fs::write(&source, payload).unwrap();
        let directory = root.path().join("code");
        run("create-directory", |operation| {
            operation.queue_create_directory(&directory)
        })
        .unwrap();
        let incoming = directory.join("payload.lanpath-staging");
        run("copy-file", |operation| {
            operation.queue_copy_file(&source, &incoming)
        })
        .unwrap();
        assert!(
            run("copy-collision", |operation| operation
                .queue_copy_file(&source, &incoming))
            .is_err()
        );
        assert_eq!(fs::read(&incoming).unwrap(), payload);
        let final_file = directory.join("payload.bin");
        fs::write(&final_file, b"existing final").unwrap();
        assert!(
            run("rename-collision", |operation| operation
                .queue_rename_file(&incoming, &final_file))
            .is_err()
        );
        assert_eq!(fs::read(&final_file).unwrap(), b"existing final");
        assert_eq!(fs::read(&incoming).unwrap(), payload);
        fs::remove_file(&final_file).unwrap();
        run("rename-file", |operation| {
            operation.queue_rename_file(&incoming, &final_file)
        })
        .unwrap();
        assert!(!incoming.try_exists().unwrap());
        assert_eq!(fs::read(&final_file).unwrap(), payload);
        assert!(
            run("reject-directory-delete", |operation| operation
                .queue_delete_files([&directory]))
            .is_err()
        );
        run("delete-file", |operation| {
            operation.queue_delete_files([&final_file])
        })
        .unwrap();
        assert!(!final_file.try_exists().unwrap());
        assert_eq!(fs::read(&source).unwrap(), payload);
        // A second MTA initialization must still succeed on the caller thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .unwrap();
        let _unchanged_caller = Apartment(PhantomData);
    })
    .join()
    .unwrap();
}

#[test]
fn one_shell_session_stages_nested_files_while_retaining_directory_and_source_leases() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.bin");
    let payload = b"immutable source\0\xff";
    fs::write(&source, payload).unwrap();
    let _source = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&source)
        .unwrap();
    session(|operation| {
        let mut directories = crate::windows_file::DirectoryLocks::acquire(root.path()).unwrap();
        let code = root.path().join(format!(
            "LanpathGPUI-Helper-{}-{}",
            "a".repeat(64),
            "b".repeat(64)
        ));
        operation
            .batch("create-code", |operation| {
                operation.queue_create_directory(&code)
            })
            .unwrap();
        directories.extend(&code).unwrap();
        let resources = code.join("resources");
        operation
            .batch("create-resources", |operation| {
                operation.queue_create_directory(&resources)
            })
            .unwrap();
        directories.extend(&resources).unwrap();
        let seed = resources.join("seed");
        operation
            .batch("create-seed", |operation| {
                operation.queue_create_directory(&seed)
            })
            .unwrap();
        directories.extend(&seed).unwrap();
        // Signed bundle inventories use portable slash-separated relative paths.
        let incoming = code.join("resources/seed/payload.lanpath-staging");
        operation
            .batch("copy-frozen-source", |operation| {
                operation.queue_copy_file(&source, &incoming)
            })
            .unwrap();
        assert_eq!(fs::read(&incoming).unwrap(), payload);
        let promoted = code.join("resources/seed/payload.bin");
        operation
            .batch("promote-file", |operation| {
                operation.queue_rename_file(&incoming, &promoted)
            })
            .unwrap();
        assert_eq!(fs::read(&promoted).unwrap(), payload);
        assert!(!incoming.try_exists().unwrap());
        Ok(())
    })
    .unwrap();
}

#[test]
#[ignore = "requires authenticated LANPATH_TEST_STAGE_IMAGE and explicit Windows administrator consent"]
fn authenticated_bundle_stages_to_a_non_user_writable_code_cache() {
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let image = std::path::PathBuf::from(
        std::env::var_os("LANPATH_TEST_STAGE_IMAGE").expect("set authenticated native stage image"),
    );
    crate::windows_gui::ensure_unelevated().unwrap();
    let source = crate::privileged::staging::trusted_bundle_at(&image).unwrap();
    let identity =
        crate::privileged::security::token_identity(unsafe { GetCurrentProcess() }).unwrap();
    let protected = crate::privileged::broker_staging::ensure(&source, &identity.sid).unwrap();
    assert_ne!(protected.root, source.root);
    let protected_image = protected.root.join(crate::product::BINARY_NAME);
    drop(protected);
    let write = fs::OpenOptions::new().write(true).open(protected_image);
    assert_eq!(
        write.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    println!(
        "authenticated protected bundle staged; ordinary-user write denied after all validation leases were released"
    );
}
