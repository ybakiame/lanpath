use super::wire::{self, Request, Response};
use lanpath_client::sysproxy::SystemProxySession;
use lanpath_common::LanpathResult;
use std::io::{Read, Write};

/// The guardian, not the GUI/RPC caller, owns every mutation and checkpoint. A lost
/// caller never interrupts the synchronous session operation; both failed request
/// reads and failed response writes enter the same exact-state restoration path.
pub(super) fn serve(
    session: &SystemProxySession,
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
                .acquire((std::net::Ipv4Addr::LOCALHOST, port).into())
                .map(|()| Response::Acquired),
            Request::Status => session.status().map(Response::Status),
            Request::Stop => session.restore().map(|()| Response::Stopped),
            Request::RecoverAbandoned => session.recover_abandoned().map(|_| Response::Recovered),
            Request::RecoverWatched => session
                .recover_abandoned_owner(session.owner_id())
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
