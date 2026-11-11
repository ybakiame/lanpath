#![cfg(windows)]

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible,
        SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_CLOSE, WM_NULL,
    },
};

struct OwnedGui(Child);
impl Drop for OwnedGui {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
struct Search {
    pid: u32,
    window: HWND,
}
unsafe extern "system" fn find_owned_window(window: HWND, parameter: LPARAM) -> i32 {
    let search = unsafe { &mut *(parameter as *mut Search) };
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(window, &mut pid);
    }
    if pid == search.pid
        && unsafe { IsWindowVisible(window) } != 0
        && unsafe { GetWindow(window, GW_OWNER) }.is_null()
    {
        search.window = window;
        return 0;
    }
    1
}

/// Opt-in real window-system regression, not a new default CI gate. The child
/// has an isolated data root and never starts a core or changes system networking.
#[test]
#[ignore = "requires LANPATH_TEST_GPUI_BINARY pointing to the actual Windows GUI executable"]
fn fresh_window_processes_messages_and_completes_graceful_close() {
    let binary = PathBuf::from(
        std::env::var_os("LANPATH_TEST_GPUI_BINARY").expect("set the actual GPUI executable"),
    );
    assert!(binary.is_absolute() && binary.is_file());
    let root = tempfile::tempdir().unwrap();
    let stderr = root.path().join("stderr.log");
    let mut gui = OwnedGui(
        Command::new(&binary)
            .current_dir(binary.parent().unwrap())
            .env("LANPATH_DESKTOP_DATA_DIR", root.path().join("userdata"))
            .env_remove("LANPATH_DESKTOP_CORE_BINARY")
            .stdout(Stdio::null())
            .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()))
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    let window = loop {
        if let Some(status) = gui.0.try_wait().unwrap() {
            panic!(
                "GUI exited during startup: {status}; {}",
                std::fs::read_to_string(&stderr).unwrap()
            );
        }
        let mut search = Search {
            pid: gui.0.id(),
            window: std::ptr::null_mut(),
        };
        unsafe {
            EnumWindows(
                Some(find_owned_window),
                (&mut search as *mut Search) as LPARAM,
            );
        }
        if !search.window.is_null() {
            break search.window;
        }
        assert!(
            Instant::now() < deadline,
            "owned GUI did not create a visible window"
        );
        thread::sleep(Duration::from_millis(50));
    };
    let mut result = 0;
    assert_ne!(
        unsafe {
            SendMessageTimeoutW(
                window,
                WM_NULL,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                2000,
                &mut result,
            )
        },
        0,
        "visible GUI stopped processing messages"
    );
    assert_ne!(
        unsafe {
            SendMessageTimeoutW(
                window,
                WM_CLOSE,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                2000,
                &mut result,
            )
        },
        0,
        "visible GUI did not accept close"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = gui.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "graceful GUI shutdown failed: {status}; {}",
                std::fs::read_to_string(&stderr).unwrap()
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "GUI accepted close but its foreground event loop did not complete shutdown"
        );
        thread::sleep(Duration::from_millis(50));
    }
}
