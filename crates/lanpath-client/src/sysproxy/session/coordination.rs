use lanpath_common::{LanpathError, LanpathResult};
use std::time::{Duration, Instant};

/// Wait only for the existing operation-file sharing lock, never an owner lease or
/// a rejected journal/snapshot. All old file-lock consumers remain serialized.
pub(crate) fn wait<T>(
    mut open: impl FnMut() -> LanpathResult<T>,
    timeout: Duration,
) -> LanpathResult<T> {
    let deadline = Instant::now() + timeout;
    loop {
        match open() {
            Err(LanpathError::Io(error))
                if matches!(error.raw_os_error(), Some(32 | 33)) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_both_file_contention_errors_then_acquires_same_operation() {
        let mut attempts = 0;
        let result = wait(
            || {
                attempts += 1;
                match attempts {
                    1 => Err(std::io::Error::from_raw_os_error(32).into()),
                    2 => Err(std::io::Error::from_raw_os_error(33).into()),
                    _ => Ok(42),
                }
            },
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(result, 42);
        assert_eq!(attempts, 3);
    }

    #[test]
    fn permission_refusal_and_expired_wait_are_never_retried() {
        for code in [5, 32, 33] {
            let mut attempts = 0;
            let result: LanpathResult<()> = wait(
                || {
                    attempts += 1;
                    Err(std::io::Error::from_raw_os_error(code).into())
                },
                Duration::ZERO,
            );
            assert!(result.is_err());
            assert_eq!(attempts, 1);
        }
        let mut attempts = 0;
        let result: LanpathResult<()> = wait(
            || {
                attempts += 1;
                Err(LanpathError::Client(
                    "external snapshot/active owner refusal".into(),
                ))
            },
            Duration::from_secs(1),
        );
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }
}
