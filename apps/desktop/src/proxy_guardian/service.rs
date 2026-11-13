#![cfg_attr(not(windows), allow(dead_code))]

use super::{
    session::GuardianSession,
    wire::{self, Request, Response},
};
use lanpath_common::LanpathResult;
use std::io::{Read, Write};

/// The guardian, not the GUI/RPC caller, owns every mutation and checkpoint. A lost
/// caller never interrupts the synchronous session operation; both failed request
/// reads and failed response writes enter the same exact-state restoration path.
pub(super) fn serve(
    session: &dyn GuardianSession,
    pipe: &mut (impl Read + Write),
) -> LanpathResult<()> {
    let mut sequence = 1u64;
    loop {
        let request = match wire::read_frame::<Request>(pipe, sequence) {
            Ok(request) => request,
            Err(_) => return session.restore(),
        };
        let result = match request {
            Request::Acquire(port) => session
                .acquire(
                    port,
                    lanpath_client::sysproxy::AcquisitionPolicy::RequireDirect,
                )
                .map(|()| Response::Acquired),
            Request::AcquireTakeover(port) => session
                .acquire(
                    port,
                    lanpath_client::sysproxy::AcquisitionPolicy::ExplicitTakeover,
                )
                .map(|()| Response::Acquired),
            Request::Status => session.status().map(Response::Status),
            Request::Stop => session.restore().map(|()| Response::Stopped),
            Request::RecoverAbandoned => session.recover_abandoned().map(|_| Response::Recovered),
            Request::RecoverWatched => session
                .recover_watched(session.owner_id())
                .map(|_| Response::Recovered),
        };
        let stopping = matches!(&result, Ok(Response::Stopped));
        let response = result.unwrap_or_else(|error| Response::Error(error.to_string()));
        if wire::write_frame(pipe, sequence, &response).is_err() {
            return session.restore();
        }
        if stopping {
            return Ok(());
        }
        sequence = match sequence.checked_add(1) {
            Some(sequence) => sequence,
            None => return session.restore(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanpath_client::sysproxy::{AcquisitionPolicy, SystemProxyOwnership};
    use lanpath_common::LanpathError;
    use std::collections::VecDeque;
    use std::sync::{Arc, Condvar, Mutex};

    /// 共享 session 契约的 fixture：记录操作序列，可注入失败。对齐
    /// `lanpath-client` sysproxy session 测试 fixture 的语义；它是
    /// fixture，不是 Windows 真实 WinINet。
    #[derive(Default)]
    struct FakeState {
        calls: Vec<String>,
        fail: Option<&'static str>,
    }
    struct FakeSession {
        state: Arc<Mutex<FakeState>>,
        ownership: SystemProxyOwnership,
    }
    impl FakeSession {
        fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(FakeState::default())),
                ownership: SystemProxyOwnership::Unowned,
            }
        }
        fn record(&self, call: String) -> LanpathResult<()> {
            let mut state = self.state.lock().unwrap();
            state.calls.push(call);
            if state.fail.is_some() {
                return Err(LanpathError::Client("fixture refusal".into()));
            }
            Ok(())
        }
    }

    impl GuardianSession for FakeSession {
        fn acquire(&self, port: u16, policy: AcquisitionPolicy) -> LanpathResult<()> {
            self.record(format!("acquire:{port}:{policy:?}"))
        }
        fn status(&self) -> LanpathResult<SystemProxyOwnership> {
            self.record("status".into())?;
            Ok(self.ownership.clone())
        }
        fn restore(&self) -> LanpathResult<()> {
            self.record("restore".into())
        }
        fn recover_abandoned(&self) -> LanpathResult<bool> {
            self.record("recover_abandoned".into())?;
            Ok(true)
        }
        fn recover_watched(&self, owner: &str) -> LanpathResult<bool> {
            self.record(format!("recover_watched:{owner}"))?;
            Ok(true)
        }
        fn owner_id(&self) -> &str {
            "fixture-owner"
        }
    }

    /// 内存双工管道：client 写 → server 读；server 写 → client 读。
    /// 读在数据到达前阻塞；close 模拟 GUI 死亡导致的管道断开（EOF）。
    struct Link {
        read: Arc<PipeState>,
        write: Arc<PipeState>,
    }
    struct PipeState {
        inner: Mutex<PipeInner>,
        ready: Condvar,
    }
    struct PipeInner {
        buf: VecDeque<u8>,
        closed: bool,
    }
    impl Link {
        fn pair() -> (Link, Link) {
            let a = Arc::new(PipeState {
                inner: Mutex::new(PipeInner {
                    buf: VecDeque::new(),
                    closed: false,
                }),
                ready: Condvar::new(),
            });
            let b = Arc::new(PipeState {
                inner: Mutex::new(PipeInner {
                    buf: VecDeque::new(),
                    closed: false,
                }),
                ready: Condvar::new(),
            });
            (
                Link {
                    read: a.clone(),
                    write: b.clone(),
                },
                Link { read: b, write: a },
            )
        }
        fn close(&self) {
            for state in [&self.read, &self.write] {
                let mut inner = state.inner.lock().unwrap();
                inner.closed = true;
                state.ready.notify_all();
            }
        }
    }
    impl Read for Link {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            let mut inner = self.read.inner.lock().unwrap();
            while inner.buf.is_empty() && !inner.closed {
                inner = self.read.ready.wait(inner).unwrap();
            }
            if inner.buf.is_empty() {
                return Ok(0); // EOF：对端（GUI）死亡/断开。
            }
            let count = bytes.len().min(inner.buf.len());
            for slot in &mut bytes[..count] {
                *slot = inner.buf.pop_front().unwrap();
            }
            Ok(count)
        }
    }
    impl Write for Link {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let mut inner = self.write.inner.lock().unwrap();
            if inner.closed {
                return Err(std::io::Error::other("fixture: peer closed"));
            }
            inner.buf.extend(bytes.iter().copied());
            self.write.ready.notify_all();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn request(client: &mut Link, sequence: u64, request: &Request) {
        wire::write_frame(client, sequence, request).unwrap();
    }
    fn response(server: &mut Link, sequence: u64) -> Response {
        wire::read_frame(server, sequence).unwrap()
    }

    #[test]
    fn serves_acquire_status_and_stop_in_order() {
        let session = FakeSession::new();
        let (mut gui, mut guardian) = Link::pair();
        let state = session.state.clone();
        let worker = std::thread::spawn(move || serve(&session, &mut guardian));
        request(&mut gui, 1, &Request::Acquire(17899));
        assert!(matches!(response(&mut gui, 1), Response::Acquired));
        request(&mut gui, 2, &Request::Status);
        assert!(matches!(
            response(&mut gui, 2),
            Response::Status(SystemProxyOwnership::Unowned)
        ));
        request(&mut gui, 3, &Request::Stop);
        assert!(matches!(response(&mut gui, 3), Response::Stopped));
        // Stop 之后服务循环退出：不再读取后续帧。
        worker.join().unwrap().unwrap();
        assert_eq!(
            state.lock().unwrap().calls,
            vec!["acquire:17899:RequireDirect", "status", "restore"]
        );
    }

    #[test]
    fn takeover_request_maps_to_explicit_takeover_policy_only() {
        let session = FakeSession::new();
        let (mut gui, mut guardian) = Link::pair();
        let state = session.state.clone();
        let worker = std::thread::spawn(move || serve(&session, &mut guardian));
        // 普通 Acquire 恒为 RequireDirect——伪造的普通请求不是接管授权。
        request(&mut gui, 1, &Request::Acquire(17899));
        assert!(matches!(response(&mut gui, 1), Response::Acquired));
        // 显式接管授权形态映射 ExplicitTakeover。
        request(&mut gui, 2, &Request::AcquireTakeover(17900));
        assert!(matches!(response(&mut gui, 2), Response::Acquired));
        request(&mut gui, 3, &Request::Stop);
        assert!(matches!(response(&mut gui, 3), Response::Stopped));
        worker.join().unwrap().unwrap();
        assert_eq!(
            state.lock().unwrap().calls,
            vec![
                "acquire:17899:RequireDirect",
                "acquire:17900:ExplicitTakeover",
                "restore"
            ]
        );
    }

    #[test]
    fn refusal_is_reported_but_stream_stays_aligned() {
        let session = FakeSession::new();
        let (mut gui, mut guardian) = Link::pair();
        session.state.lock().unwrap().fail = Some("acquire");
        let state = session.state.clone();
        let worker = std::thread::spawn(move || serve(&session, &mut guardian));
        request(&mut gui, 1, &Request::Acquire(17899));
        match response(&mut gui, 1) {
            Response::Error(message) => assert!(message.contains("fixture refusal")),
            other => panic!("unexpected response: {other:?}"),
        }
        // 明确的拒绝不改变顺序：清除失败后续帧按 2 继续。
        state.lock().unwrap().fail = None;
        request(&mut gui, 2, &Request::Status);
        assert!(matches!(response(&mut gui, 2), Response::Status(_)));
        request(&mut gui, 3, &Request::Stop);
        assert!(matches!(response(&mut gui, 3), Response::Stopped));
        worker.join().unwrap().unwrap();
        assert_eq!(
            state.lock().unwrap().calls,
            vec!["acquire:17899:RequireDirect", "status", "restore"]
        );
    }

    #[test]
    fn gui_disconnect_mid_session_triggers_exact_restore() {
        let session = FakeSession::new();
        let (mut gui, mut guardian) = Link::pair();
        let state = session.state.clone();
        let worker = std::thread::spawn(move || serve(&session, &mut guardian));
        request(&mut gui, 1, &Request::Acquire(17899));
        assert!(matches!(response(&mut gui, 1), Response::Acquired));
        // GUI 死亡：管道断开，守护必须完成恢复（一次 restore）。
        gui.close();
        worker.join().unwrap().unwrap();
        assert_eq!(
            state.lock().unwrap().calls,
            vec!["acquire:17899:RequireDirect", "restore"]
        );
    }

    #[test]
    fn wrong_sequence_or_version_from_gui_is_not_a_receipt() {
        let session = FakeSession::new();
        let (mut gui, mut guardian) = Link::pair();
        let state = session.state.clone();
        let worker = std::thread::spawn(move || serve(&session, &mut guardian));
        // 乱序帧（2 抢先）触发守护按未知状态恢复并退出。
        request(&mut gui, 2, &Request::Status);
        worker.join().unwrap().unwrap();
        assert_eq!(state.lock().unwrap().calls, vec!["restore"]);
    }

    #[test]
    fn recover_watched_targets_only_the_authenticated_owner() {
        let session = FakeSession::new();
        let (mut gui, mut guardian) = Link::pair();
        let state = session.state.clone();
        let worker = std::thread::spawn(move || serve(&session, &mut guardian));
        request(&mut gui, 1, &Request::RecoverWatched);
        assert!(matches!(response(&mut gui, 1), Response::Recovered));
        request(&mut gui, 2, &Request::Stop);
        assert!(matches!(response(&mut gui, 2), Response::Stopped));
        worker.join().unwrap().unwrap();
        assert_eq!(
            state.lock().unwrap().calls,
            vec!["recover_watched:fixture-owner", "restore"]
        );
    }

    #[test]
    fn recover_abandoned_and_status_have_distinct_paths() {
        let session = FakeSession::new();
        let (mut gui, mut guardian) = Link::pair();
        let state = session.state.clone();
        let worker = std::thread::spawn(move || serve(&session, &mut guardian));
        request(&mut gui, 1, &Request::RecoverAbandoned);
        assert!(matches!(response(&mut gui, 1), Response::Recovered));
        request(&mut gui, 2, &Request::Status);
        assert!(matches!(response(&mut gui, 2), Response::Status(_)));
        request(&mut gui, 3, &Request::Stop);
        assert!(matches!(response(&mut gui, 3), Response::Stopped));
        worker.join().unwrap().unwrap();
        assert_eq!(
            state.lock().unwrap().calls,
            vec!["recover_abandoned", "status", "restore"]
        );
    }
}
