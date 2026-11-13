#![cfg_attr(not(windows), allow(dead_code))]

use super::{
    protocol::failure,
    wire::{self, Request, Response},
};
use lanpath_common::LanpathResult;
use std::io::{Read, Write};

pub(super) struct Client<T> {
    transport: Option<T>,
    sequence: u64,
}
impl<T: Read + Write> Client<T> {
    fn new(transport: T) -> Self {
        Self {
            transport: Some(transport),
            sequence: 0,
        }
    }
    pub fn accept_ready(mut transport: T, owner: &str) -> LanpathResult<Self> {
        match wire::read_frame::<Response>(&mut transport, 0)? {
            Response::Ready(actual) if actual == owner => Ok(Self::new(transport)),
            _ => Err(failure("guardian readiness/owner identity mismatch")),
        }
    }
    pub fn transport(&mut self) -> LanpathResult<&mut T> {
        self.transport
            .as_mut()
            .ok_or_else(|| failure("guardian connection is permanently closed"))
    }
    pub fn connected(&self) -> bool {
        self.transport.is_some()
    }

    pub fn request(&mut self, request: Request) -> LanpathResult<Response> {
        let sequence = match self.sequence.checked_add(1) {
            Some(sequence) => sequence,
            None => {
                self.transport = None;
                return Err(failure("guardian sequence overflow"));
            }
        };
        self.sequence = sequence;
        let result = (|| {
            let transport = self.transport()?;
            wire::write_frame(transport, sequence, &request)?;
            let response = wire::read_frame(transport, sequence)?;
            match (&request, &response) {
                (_, Response::Error(_))
                | (Request::Acquire(_), Response::Acquired)
                | (Request::AcquireTakeover(_), Response::Acquired)
                | (Request::Status, Response::Status(_))
                | (Request::Stop, Response::Stopped) => Ok(response),
                (Request::RecoverAbandoned | Request::RecoverWatched, Response::Recovered) => {
                    Ok(response)
                }
                _ => Err(failure("unexpected guardian response")),
            }
        })();
        match result {
            Ok(Response::Error(message)) => Err(failure(message)), // Known refusal, stream still aligned.
            Ok(response) => Ok(response),
            Err(error) => {
                // Partial frames, timeouts, sequence/version errors and decode failures
                // irreversibly close the pipe. Guardian finishes the operation and restores.
                self.transport = None;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    struct Transport {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl Read for Transport {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(bytes)
        }
    }
    impl Write for Transport {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.output.write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn client(responses: &[(u64, Response)]) -> Client<Transport> {
        let mut input = Vec::new();
        for (sequence, response) in responses {
            wire::write_frame(&mut input, *sequence, response).unwrap();
        }
        Client::new(Transport {
            input: Cursor::new(input),
            output: Vec::new(),
        })
    }
    #[test]
    fn late_acquire_cannot_be_reused_as_stop_receipt() {
        let mut client = client(&[(0, Response::Acquired), (2, Response::Stopped)]);
        assert!(client.request(Request::Acquire(17899)).is_err());
        assert!(!client.connected());
        assert!(client.transport().is_err());
        assert!(client.request(Request::Stop).is_err());
    }
    #[test]
    fn explicit_refusal_keeps_framing_but_wrong_response_shape_closes_it() {
        let mut client = client(&[
            (1, Response::Error("external edit".into())),
            (
                2,
                Response::Status(lanpath_client::sysproxy::SystemProxyOwnership::Owned),
            ),
            (3, Response::Acquired),
        ]);
        assert!(client.request(Request::Stop).is_err());
        assert!(client.connected());
        assert!(matches!(
            client.request(Request::Status).unwrap(),
            Response::Status(_)
        ));
        assert!(client.request(Request::Stop).is_err());
        assert!(!client.connected());
    }
    #[test]
    fn no_rpc_can_be_issued_without_matching_readiness_frame() {
        for response in [
            Response::Ready("wrong owner".into()),
            Response::Acquired,
            Response::Error("startup failed".into()),
        ] {
            let mut input = Vec::new();
            wire::write_frame(&mut input, 0, &response).unwrap();
            let transport = Transport {
                input: Cursor::new(input),
                output: Vec::new(),
            };
            assert!(Client::accept_ready(transport, "watched owner").is_err());
        }
        let transport = Transport {
            input: Cursor::new(Vec::new()),
            output: Vec::new(),
        };
        assert!(Client::accept_ready(transport, "watched owner").is_err());
    }
    #[test]
    fn takeover_acquire_has_its_own_request_shape_and_receipt() {
        let mut first = client(&[(1, Response::Acquired)]);
        assert!(matches!(
            first.request(Request::AcquireTakeover(17899)).unwrap(),
            Response::Acquired
        ));
        // 普通 Acquire 与接管授权分开计费：序列仍单调，迟到回执不可复用。
        let mut late = client(&[(2, Response::Acquired)]);
        assert!(late.request(Request::AcquireTakeover(17899)).is_err());
        assert!(!late.connected());
    }
}
