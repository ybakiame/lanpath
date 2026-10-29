use super::ProcessIdentity;
use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt, path::PathBuf};

pub(super) fn capture(pid: u32) -> io::Result<ProcessIdentity> {
    let native_pid = i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid process PID"))?;
    let created = creation_identity(native_pid)?;
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length =
        unsafe { libc::proc_pidpath(native_pid, path.as_mut_ptr().cast(), path.len() as u32) };
    if length <= 0 {
        return Err(query_error(io::Error::last_os_error()));
    }
    let length = length as usize;
    if length >= path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid process path size",
        ));
    }
    let bytes = &path[..length];
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    if end == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "empty process path",
        ));
    }
    if creation_identity(native_pid)? != created {
        return Err(io::Error::other("process identity changed during capture"));
    }
    Ok(ProcessIdentity {
        pid,
        created,
        exe: PathBuf::from(OsStr::from_bytes(&bytes[..end])),
    })
}

fn creation_identity(pid: i32) -> io::Result<u64> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let expected = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let size = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            expected,
        )
    };
    if size <= 0 {
        return Err(query_error(io::Error::last_os_error()));
    }
    if size != expected || info.pbi_pid != pid as u32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "incomplete or mismatched process identity",
        ));
    }
    start_identity(info.pbi_start_tvsec, info.pbi_start_tvusec)
}

fn start_identity(seconds: u64, microseconds: u64) -> io::Result<u64> {
    if microseconds >= 1_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid process start microseconds",
        ));
    }
    seconds
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_add(microseconds))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "process start identity overflow",
            )
        })
}

fn query_error(error: io::Error) -> io::Error {
    if error.raw_os_error() == Some(libc::ESRCH) {
        io::Error::new(io::ErrorKind::NotFound, error)
    } else {
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_identity_preserves_microseconds_and_checks_bounds() {
        assert_eq!(start_identity(42, 123_456).unwrap(), 42_123_456);
        assert_ne!(
            start_identity(42, 1).unwrap(),
            start_identity(42, 2).unwrap()
        );
        assert!(start_identity(42, 1_000_000).is_err());
        assert!(start_identity(u64::MAX, 0).is_err());
        let seconds = u64::MAX / 1_000_000;
        assert!(start_identity(seconds, u64::MAX % 1_000_000 + 1).is_err());
    }

    #[test]
    fn only_absence_becomes_not_found() {
        assert_eq!(
            query_error(io::Error::from_raw_os_error(libc::ESRCH)).kind(),
            io::ErrorKind::NotFound
        );
        for code in [libc::EPERM, libc::EACCES, libc::EINVAL, libc::EIO] {
            let error = query_error(io::Error::from_raw_os_error(code));
            assert_eq!(error.raw_os_error(), Some(code));
            assert_ne!(error.kind(), io::ErrorKind::NotFound);
        }
    }
}
