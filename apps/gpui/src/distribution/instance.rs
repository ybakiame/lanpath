use super::invalid;
use crate::data_root::{DataRoot, validate_owned_root};
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const ENDPOINT: &str = ".gpui-activation.json";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activation {
    ShowWindow,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    schema: u32,
    port: u16,
    token: String,
}
/// Keep this server and the DataRoot lease alive for the complete window lifetime.
pub(crate) struct InstanceServer {
    _root: Arc<DataRoot>,
    _activation_lock: std::fs::File,
    endpoint: PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    events: mpsc::Receiver<Activation>,
}
impl InstanceServer {
    pub(crate) fn bind(root: Arc<DataRoot>) -> LanpathResult<Self> {
        let activation_lock = root.lock_activation()?;
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let endpoint = root.path().join(ENDPOINT);
        let descriptor = Endpoint {
            schema: 1,
            port: listener.local_addr()?.port(),
            token: token.clone(),
        };
        let (sender, events) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("gpui-activation".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, peer)) if peer.ip().is_loopback() => {
                            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                            let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                            let mut request = [0u8; 70];
                            if stream.read_exact(&mut request).is_ok()
                                && request[..64] == *token.as_bytes()
                                && &request[64..] == b" SHOW\n"
                                && sender.try_send(Activation::ShowWindow).is_ok()
                            {
                                let _ = stream.write_all(b"OK\n");
                            }
                        }
                        Ok(_) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(20))
                        }
                        Err(_) => break,
                    }
                }
            })?;
        let mut temporary = tempfile::NamedTempFile::new_in(root.path())?;
        serde_json::to_writer(&mut temporary, &descriptor)?;
        temporary.as_file().sync_all()?;
        if let Err(error) = temporary.persist(&endpoint) {
            stop.store(true, Ordering::Release);
            let _ = worker.join();
            return Err(error.error.into());
        }
        Ok(Self {
            _root: root,
            _activation_lock: activation_lock,
            endpoint,
            stop,
            worker: Some(worker),
            events,
        })
    }
    pub(crate) fn try_recv(&self) -> Option<Activation> {
        self.events.try_recv().ok()
    }
}
impl Drop for InstanceServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = std::fs::remove_file(&self.endpoint);
    }
}
pub(crate) fn activate_existing(root: &Path) -> LanpathResult<()> {
    let root = validate_owned_root(root)?;
    // An endpoint is authoritative only while its owning process holds the lease.
    if !DataRoot::is_in_use(&root)? {
        return Err(invalid("no running owner holds this data root"));
    }
    let metadata = std::fs::metadata(root.join(ENDPOINT))?;
    if metadata.len() > 1024 {
        return Err(invalid("activation descriptor is oversized"));
    }
    let endpoint: Endpoint = serde_json::from_slice(&std::fs::read(root.join(ENDPOINT))?)?;
    if endpoint.schema != 1
        || endpoint.port == 0
        || endpoint.token.len() != 64
        || !endpoint.token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(invalid("invalid activation descriptor"));
    }
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, endpoint.port);
    let mut stream = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(format!("{} SHOW\n", endpoint.token).as_bytes())?;
    let mut response = [0u8; 3];
    stream.read_exact(&mut response)?;
    if &response != b"OK\n" {
        return Err(invalid("activation was not acknowledged"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_owner_activates_first_and_cannot_write() {
        let dir = tempfile::tempdir().unwrap();
        let root = Arc::new(DataRoot::open(dir.path()).unwrap());
        let server = InstanceServer::bind(root.clone()).unwrap();
        assert!(DataRoot::open(dir.path()).is_err());
        activate_existing(dir.path()).unwrap();
        assert_eq!(server.try_recv(), Some(Activation::ShowWindow));
        drop(server);
        drop(root);
        assert!(activate_existing(dir.path()).is_err());
        DataRoot::open(dir.path()).unwrap();
    }
    #[test]
    fn unauthenticated_clients_cannot_activate() {
        let dir = tempfile::tempdir().unwrap();
        let root = Arc::new(DataRoot::open(dir.path()).unwrap());
        let server = InstanceServer::bind(root.clone()).unwrap();
        let endpoint: Endpoint =
            serde_json::from_slice(&std::fs::read(dir.path().join(ENDPOINT)).unwrap()).unwrap();
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port)).unwrap();
        client
            .write_all(format!("{} SHOW\n", "0".repeat(64)).as_bytes())
            .unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut response = [0; 3];
        assert!(client.read_exact(&mut response).is_err());
        assert_eq!(server.try_recv(), None);
    }
    #[test]
    fn only_one_activation_server_can_publish_for_a_lease() {
        let dir = tempfile::tempdir().unwrap();
        let root = Arc::new(DataRoot::open(dir.path()).unwrap());
        let first = InstanceServer::bind(root.clone()).unwrap();
        assert!(InstanceServer::bind(root.clone()).is_err());
        activate_existing(dir.path()).unwrap();
        assert_eq!(first.try_recv(), Some(Activation::ShowWindow));
        drop(first);
        let second = InstanceServer::bind(root).unwrap();
        activate_existing(dir.path()).unwrap();
        assert_eq!(second.try_recv(), Some(Activation::ShowWindow));
    }
}
