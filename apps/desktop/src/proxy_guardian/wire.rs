use super::protocol::failure;
use lanpath_client::sysproxy::SystemProxyOwnership;
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{Read, Write};

const MAX_FRAME: usize = 4096;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub(super) enum Request {
    Acquire(u16),
    Status,
    Stop,
    RecoverAbandoned,
    RecoverWatched,
}
#[derive(Serialize, Deserialize)]
pub(super) enum Response {
    Ready(String),
    Acquired,
    Recovered,
    Status(SystemProxyOwnership),
    Stopped,
    Error(String),
}

pub(super) fn write_frame(
    writer: &mut impl Write,
    sequence: u64,
    value: &impl Serialize,
) -> LanpathResult<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(failure("guardian frame exceeds bound"));
    }
    let mut header = [0u8; 16];
    header[..4].copy_from_slice(b"LPG1");
    header[4..12].copy_from_slice(&sequence.to_le_bytes());
    header[12..].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    writer.write_all(&header)?;
    writer.write_all(&bytes)?;
    Ok(())
}

pub(super) fn read_frame<T: DeserializeOwned>(
    reader: &mut impl Read,
    sequence: u64,
) -> LanpathResult<T> {
    let mut header = [0u8; 16];
    reader.read_exact(&mut header)?;
    if &header[..4] != b"LPG1" || header[4..12] != sequence.to_le_bytes() {
        return Err(failure("guardian frame version/sequence mismatch"));
    }
    let length = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(failure("guardian frame exceeds bound"));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn rejects_late_response_wrong_version_oversized_and_partial_frames() {
        let mut frame = Vec::new();
        write_frame(&mut frame, 1, &Response::Acquired).unwrap();
        assert!(read_frame::<Response>(&mut Cursor::new(&frame), 2).is_err());
        let mut invalid = frame.clone();
        invalid[3] = b'2';
        assert!(read_frame::<Response>(&mut Cursor::new(invalid), 1).is_err());
        let mut invalid = frame.clone();
        invalid[12..16].copy_from_slice(&4097u32.to_le_bytes());
        assert!(read_frame::<Response>(&mut Cursor::new(invalid), 1).is_err());
        for cut in [1, 15, frame.len() - 1] {
            assert!(read_frame::<Response>(&mut Cursor::new(&frame[..cut]), 1).is_err());
        }
        assert!(matches!(
            read_frame::<Response>(&mut Cursor::new(frame), 1).unwrap(),
            Response::Acquired
        ));
    }
}
