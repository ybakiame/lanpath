use super::protocol::failure;
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use uuid::Uuid;

pub(super) const VERSION: u32 = 1;
const MAX_FRAME: usize = 16 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Event {
    Hello,
    AdapterCreated { guid: Uuid, luid: u64, index: u32 },
    Firewall,
    Ready,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Packet {
    version: u32,
    sequence: u64,
    nonce: Uuid,
    event: Event,
}
#[derive(Serialize)]
struct Acknowledgement {
    version: u32,
    sequence: u64,
    nonce: Uuid,
    accepted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Hello,
    Adapter,
    Firewall,
    Ready,
    Complete,
}

pub(super) struct Handshake {
    nonce: Uuid,
    adapter_guid: Uuid,
    sequence: u64,
    firewall_needed: bool,
    phase: Phase,
    pending: bool,
}
impl Handshake {
    pub fn new(nonce: Uuid, adapter_guid: Uuid, firewall_needed: bool) -> LanpathResult<Self> {
        if nonce.is_nil() || adapter_guid.is_nil() {
            return Err(failure("managed core identities must not be nil"));
        }
        Ok(Self {
            nonce,
            adapter_guid,
            sequence: 1,
            firewall_needed,
            phase: Phase::Hello,
            pending: false,
        })
    }
    pub fn receive(&mut self, reader: &mut impl Read) -> LanpathResult<Event> {
        if self.pending || self.phase == Phase::Complete {
            return Err(failure(
                "managed core event requires the previous durable acknowledgement",
            ));
        }
        let mut size = [0u8; 4];
        reader.read_exact(&mut size)?;
        let length = u32::from_le_bytes(size) as usize;
        if length == 0 || length > MAX_FRAME {
            return Err(failure("managed core frame exceeds bound"));
        }
        let mut payload = vec![0; length];
        reader.read_exact(&mut payload)?;
        let packet: Packet = serde_json::from_slice(&payload)?;
        if packet.version != VERSION
            || packet.sequence != self.sequence
            || packet.nonce != self.nonce
        {
            return Err(failure("managed core protocol/sequence/session mismatch"));
        }
        self.phase = match (&packet.event, self.phase) {
            (Event::Hello, Phase::Hello) => Phase::Adapter,
            (Event::AdapterCreated { guid, luid, index }, Phase::Adapter)
                if *guid == self.adapter_guid && *luid != 0 && *index != 0 =>
            {
                if self.firewall_needed {
                    Phase::Firewall
                } else {
                    Phase::Ready
                }
            }
            (Event::Firewall, Phase::Firewall) => Phase::Ready,
            (Event::Ready, Phase::Ready) => Phase::Complete,
            _ => {
                return Err(failure(
                    "managed core event is out of order or does not identify its owned adapter",
                ));
            }
        };
        self.pending = true;
        Ok(packet.event)
    }
    /// Caller persists the validated event/owned effects before invoking this method.
    pub fn acknowledge(&mut self, writer: &mut impl Write) -> LanpathResult<()> {
        if !self.pending {
            return Err(failure("no validated managed core event to acknowledge"));
        }
        let next = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| failure("managed core sequence exhausted"))?;
        let bytes = serde_json::to_vec(&Acknowledgement {
            version: VERSION,
            sequence: self.sequence,
            nonce: self.nonce,
            accepted: true,
        })?;
        writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
        writer.write_all(&bytes)?;
        writer.flush()?;
        self.sequence = next;
        self.pending = false;
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.phase == Phase::Complete && !self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(nonce: Uuid, sequence: u64, event: Event) -> Vec<u8> {
        let data = serde_json::to_vec(&Packet {
            version: VERSION,
            sequence,
            nonce,
            event,
        })
        .unwrap();
        let mut bytes = (data.len() as u32).to_le_bytes().to_vec();
        bytes.extend(data);
        bytes
    }
    #[test]
    fn readiness_requires_owned_adapter_and_every_durable_side_effect_acknowledgement() {
        for firewall in [false, true] {
            let nonce = Uuid::new_v4();
            let adapter = Uuid::new_v4();
            let mut handshake = Handshake::new(nonce, adapter, firewall).unwrap();
            let mut events = vec![
                Event::Hello,
                Event::AdapterCreated {
                    guid: adapter,
                    luid: 9841,
                    index: 77,
                },
            ];
            if firewall {
                events.push(Event::Firewall);
            }
            events.push(Event::Ready);
            for (index, event) in events.into_iter().enumerate() {
                let bytes = packet(nonce, index as u64 + 1, event);
                handshake.receive(&mut bytes.as_slice()).unwrap();
                assert!(!handshake.complete());
                assert!(handshake.receive(&mut bytes.as_slice()).is_err());
                handshake.acknowledge(&mut Vec::new()).unwrap();
            }
            assert!(handshake.complete());
            assert!(
                handshake
                    .receive(&mut packet(nonce, 5, Event::Ready).as_slice())
                    .is_err()
            );
        }
    }
    #[test]
    fn another_session_replay_or_forged_adapter_never_unlocks_configuration() {
        let nonce = Uuid::new_v4();
        let adapter = Uuid::new_v4();
        for (session, sequence) in [(Uuid::new_v4(), 1), (nonce, 0), (nonce, 2)] {
            let mut handshake = Handshake::new(nonce, adapter, false).unwrap();
            assert!(
                handshake
                    .receive(&mut packet(session, sequence, Event::Hello).as_slice())
                    .is_err()
            );
            assert!(!handshake.complete());
        }
        for event in [
            Event::Ready,
            Event::Firewall,
            Event::AdapterCreated {
                guid: Uuid::new_v4(),
                luid: 1,
                index: 2,
            },
            Event::AdapterCreated {
                guid: adapter,
                luid: 0,
                index: 2,
            },
            Event::AdapterCreated {
                guid: adapter,
                luid: 1,
                index: 0,
            },
        ] {
            let mut handshake = Handshake::new(nonce, adapter, false).unwrap();
            handshake
                .receive(&mut packet(nonce, 1, Event::Hello).as_slice())
                .unwrap();
            handshake.acknowledge(&mut Vec::new()).unwrap();
            assert!(
                handshake
                    .receive(&mut packet(nonce, 2, event).as_slice())
                    .is_err()
            );
            assert!(!handshake.complete());
        }
    }
}
