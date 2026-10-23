//! The replacement must not open stores until the old process releases its
//! exclusive directory lease. A private inherited pipe waits for process exit,
//! not a guessed delay; ordinary launches retain fail-fast single-instance rules.
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::Mutex,
};

const WAIT_ARGUMENT: &str = "--lanpath-restore-parent-pipe";
const HANDOFF: &[u8] = b"LPBKUP1\n";
// Statics are not dropped during Rust shutdown. The OS closes this parent's
// pipe on actual process exit, after every data-root owner and writer is gone.
static REPLACEMENT: Mutex<Option<Child>> = Mutex::new(None);

pub(crate) fn wait_for_parent() -> Result<(), String> {
    if !std::env::args_os().any(|argument| argument == WAIT_ARGUMENT) {
        return Ok(());
    }
    receive_handoff(&mut std::io::stdin().lock())
        .map_err(|error| format!("恢复重启交接失败：{error}"))
}

fn receive_handoff(input: &mut impl Read) -> std::io::Result<()> {
    let mut message = [0; HANDOFF.len()];
    input.read_exact(&mut message)?;
    if message != HANDOFF {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid restart handoff",
        ));
    }
    let mut extra = [0];
    if input.read(&mut extra)? != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unexpected restart data",
        ));
    }
    Ok(())
}

pub(crate) fn spawn_replacement() -> Result<(), String> {
    let mut replacement = REPLACEMENT
        .lock()
        .map_err(|_| "重启交接锁不可用".to_string())?;
    if replacement.is_some() {
        return Err("重启交接已开始".into());
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut child = Command::new(executable)
        .args(
            std::env::args_os()
                .skip(1)
                .filter(|argument| argument != WAIT_ARGUMENT),
        )
        .arg(WAIT_ARGUMENT)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动恢复后的实验版：{error}"))?;
    let result = child
        .stdin
        .as_mut()
        .ok_or_else(|| "无法创建恢复交接通道".to_string())
        .and_then(|pipe| pipe.write_all(HANDOFF).map_err(|error| error.to_string()));
    if let Err(error) = result {
        // Only this just-created replacement is ours to reap on failed handoff.
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("无法交接恢复重启：{error}"));
    }
    *replacement = Some(child);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_cannot_enter_application_with_incomplete_or_unexpected_handoff() {
        for message in [&b""[..], &HANDOFF[..3], b"wrong!!!", b"LPBKUP1\nunexpected"] {
            assert!(receive_handoff(&mut std::io::Cursor::new(message)).is_err());
        }
        assert!(receive_handoff(&mut std::io::Cursor::new(HANDOFF)).is_ok());
    }
}
