//! Ordinary-user, one-session WinINet crash recovery. No GUI, stores, scripts or service.
#[cfg(windows)]
mod launch;
#[cfg(any(windows, test))]
mod protocol;
#[cfg(any(windows, test))]
mod rpc;
#[cfg(windows)]
mod security;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod wire;

#[cfg(windows)]
pub(crate) use launch::Guardian;

/// Called by the binary before entering the GUI library's startup path.
pub fn dispatch_from_env() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let command = args.next()?;
    if command != "--proxy-guardian" {
        return None;
    }
    #[cfg(windows)]
    {
        let args: Vec<_> = std::iter::once(command).chain(args).collect();
        Some(
            match protocol::Arguments::parse(&args).and_then(windows::watch) {
                Ok(()) => 0,
                // A failed/refused recovery leaves the protected journal for explicit recovery.
                Err(_) => 6,
            },
        )
    }
    #[cfg(not(windows))]
    {
        Some(6)
    }
}
