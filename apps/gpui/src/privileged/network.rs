use super::{
    protocol::{AdapterReceipt, InterfaceObservation, PreparedSession, failure},
    security,
};
use lanpath_common::LanpathResult;
use std::{io, ptr};
use windows_sys::Win32::{Foundation::*, NetworkManagement::IpHelper::*, Networking::WinSock::*};

#[path = "network_presence.rs"]
mod presence;

fn status(code: u32) -> LanpathResult<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code as i32).into())
    }
}
unsafe fn address(socket: *const SOCKADDR) -> LanpathResult<String> {
    if socket.is_null() {
        return Err(failure("network observation contains a null socket"));
    }
    let family = unsafe { (*socket).sa_family };
    let data = match family {
        AF_INET => unsafe {
            &(*socket.cast::<SOCKADDR_IN>()).sin_addr as *const IN_ADDR as *const core::ffi::c_void
        },
        AF_INET6 => unsafe {
            &(*socket.cast::<SOCKADDR_IN6>()).sin6_addr as *const IN6_ADDR
                as *const core::ffi::c_void
        },
        _ => {
            return Err(failure(
                "network observation contains an unsupported address family",
            ));
        }
    };
    let mut text = [0u16; 64];
    if unsafe { InetNtopW(family as i32, data, text.as_mut_ptr(), text.len()) }.is_null() {
        return Err(io::Error::last_os_error().into());
    }
    Ok(unsafe { security::read_wide(text.as_ptr()) })
}
fn inet(value: &SOCKADDR_INET) -> LanpathResult<String> {
    unsafe { address((value as *const SOCKADDR_INET).cast()) }
}

pub(super) fn snapshot() -> LanpathResult<Vec<InterfaceObservation>> {
    let mut size = 16 * 1024u32;
    let mut storage;
    loop {
        if size > 16 * 1024 * 1024 {
            return Err(failure("adapter observation exceeds bound"));
        }
        storage = vec![0u64; (size as usize).div_ceil(8)];
        let code = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC as u32,
                GAA_FLAG_INCLUDE_ALL_INTERFACES | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST,
                ptr::null(),
                storage.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if code == ERROR_NO_DATA {
            return Ok(Vec::new());
        }
        if code == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        status(code)?;
        break;
    }
    let mut result = Vec::new();
    let mut adapter = storage.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    while !adapter.is_null() {
        if result.len() > 4096 {
            return Err(failure("adapter count exceeds observation bound"));
        }
        let item = unsafe { &*adapter };
        let mut guid = windows_sys::core::GUID::default();
        status(unsafe { ConvertInterfaceLuidToGuid(&item.Luid, &mut guid) })?;
        let mut observation = InterfaceObservation {
            name: unsafe { security::read_wide(item.FriendlyName) },
            guid: format!(
                "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                guid.data1,
                guid.data2,
                guid.data3,
                guid.data4[0],
                guid.data4[1],
                guid.data4[2],
                guid.data4[3],
                guid.data4[4],
                guid.data4[5],
                guid.data4[6],
                guid.data4[7]
            ),
            luid: unsafe { item.Luid.Value },
            index: unsafe { item.Anonymous1.Anonymous.IfIndex },
            addresses: Vec::new(),
            routes: Vec::new(),
            dns_servers: Vec::new(),
        };
        let mut unicast = item.FirstUnicastAddress;
        while !unicast.is_null() {
            let item = unsafe { &*unicast };
            observation.addresses.push(format!(
                "{}/{}",
                unsafe { address(item.Address.lpSockaddr)? },
                item.OnLinkPrefixLength
            ));
            if observation.addresses.len() > 4096 {
                return Err(failure("address count exceeds bound"));
            }
            unicast = item.Next;
        }
        let mut dns = item.FirstDnsServerAddress;
        while !dns.is_null() {
            let item = unsafe { &*dns };
            observation
                .dns_servers
                .push(unsafe { address(item.Address.lpSockaddr)? });
            if observation.dns_servers.len() > 4096 {
                return Err(failure("DNS count exceeds bound"));
            }
            dns = item.Next;
        }
        result.push(observation);
        adapter = item.Next;
    }
    let routes = routes()?;
    for item in &mut result {
        item.routes = routes
            .iter()
            .filter(|(luid, _)| *luid == item.luid)
            .map(|(_, route)| route.clone())
            .collect();
    }
    Ok(result)
}
fn routes() -> LanpathResult<Vec<(u64, String)>> {
    let mut table = ptr::null_mut();
    status(unsafe { GetIpForwardTable2(AF_UNSPEC, &mut table) })?;
    struct Table(*mut MIB_IPFORWARD_TABLE2);
    impl Drop for Table {
        fn drop(&mut self) {
            unsafe {
                FreeMibTable(self.0.cast());
            }
        }
    }
    let table = Table(table);
    if table.0.is_null() {
        return Err(failure("route observation returned a null table"));
    }
    let count = unsafe { (*table.0).NumEntries } as usize;
    if count > 1_000_000 {
        return Err(failure("route observation count exceeds bound"));
    }
    let rows = unsafe { std::slice::from_raw_parts((*table.0).Table.as_ptr(), count) };
    rows.iter()
        .map(|row| {
            Ok((
                unsafe { row.InterfaceLuid.Value },
                format!(
                    "{}/{} via {} metric {}",
                    inet(&row.DestinationPrefix.Prefix)?,
                    row.DestinationPrefix.PrefixLength,
                    inet(&row.NextHop)?,
                    row.Metric
                ),
            ))
        })
        .collect()
}

fn uuid(guid: &windows_sys::core::GUID) -> uuid::Uuid {
    uuid::Uuid::from_fields(guid.data1, guid.data2, guid.data3, &guid.data4)
}
struct NativeTable<T>(*mut T);
impl<T> Drop for NativeTable<T> {
    fn drop(&mut self) {
        unsafe {
            FreeMibTable(self.0.cast());
        }
    }
}
fn interface(row: &MIB_IF_ROW2) -> LanpathResult<InterfaceObservation> {
    let length = row
        .Alias
        .iter()
        .position(|value| *value == 0)
        .ok_or_else(|| failure("unterminated native interface alias"))?;
    let name = String::from_utf16(&row.Alias[..length])
        .map_err(|_| failure("invalid native interface alias"))?;
    Ok(InterfaceObservation {
        name,
        guid: uuid(&row.InterfaceGuid).to_string(),
        luid: unsafe { row.InterfaceLuid.Value },
        index: row.InterfaceIndex,
        addresses: Vec::new(),
        routes: Vec::new(),
        dns_servers: Vec::new(),
    })
}

/// The report is checked against the native row before configuration is acknowledged.
pub(super) fn created(
    prepared: &PreparedSession,
    receipt: &AdapterReceipt,
) -> LanpathResult<InterfaceObservation> {
    let mut row = MIB_IF_ROW2::default();
    row.InterfaceLuid.Value = receipt.luid;
    status(unsafe { GetIfEntry2(&mut row) })?;
    let observation = interface(&row)?;
    if receipt.guid != prepared.adapter_guid
        || receipt.luid == 0
        || receipt.index == 0
        || uuid(&row.InterfaceGuid) != receipt.guid
        || observation.luid != receipt.luid
        || row.InterfaceIndex != receipt.index
        || observation.name != prepared.interface_name
    {
        return Err(failure(
            "managed adapter report does not match the freshly created native GUID/LUID/index/name",
        ));
    }
    Ok(observation)
}

/// Read-only recovery evidence. Neither a name nor an observation authorizes deletion.
pub(super) fn remaining(
    prepared: &PreparedSession,
    receipt: Option<&AdapterReceipt>,
    previously: &[InterfaceObservation],
) -> LanpathResult<Vec<InterfaceObservation>> {
    let expected_guid = prepared.adapter_guid.to_string();
    let mut result: Vec<_> = snapshot()?
        .into_iter()
        .filter(|item| {
            item.name == prepared.interface_name
                || item.guid == expected_guid
                || receipt.is_some_and(|owned| item.luid == owned.luid)
        })
        .collect();
    let mut table = ptr::null_mut();
    status(unsafe { GetIfTable2(&mut table) })?;
    let table = NativeTable(table);
    if table.0.is_null() {
        return Err(failure("interface observation returned a null table"));
    }
    let count = unsafe { (*table.0).NumEntries } as usize;
    if count > 1_000_000 {
        return Err(failure("interface observation exceeds bound"));
    }
    for row in unsafe { std::slice::from_raw_parts((*table.0).Table.as_ptr(), count) } {
        let luid = unsafe { row.InterfaceLuid.Value };
        if (uuid(&row.InterfaceGuid) == prepared.adapter_guid
            || receipt.is_some_and(|owned| owned.luid == luid))
            && !result.iter().any(|item| item.luid == luid)
        {
            result.push(interface(row)?);
        }
    }
    if let Some(owned) = receipt {
        let mut orphaned = InterfaceObservation {
            name: prepared.interface_name.clone(),
            guid: expected_guid,
            luid: owned.luid,
            index: owned.index,
            addresses: Vec::new(),
            routes: Vec::new(),
            dns_servers: Vec::new(),
        };
        // Query the global tables even when GetAdaptersAddresses no longer reports the adapter.
        orphaned.routes = routes()?
            .into_iter()
            .filter(|(luid, _)| *luid == owned.luid)
            .map(|(_, value)| value)
            .collect();
        let mut addresses = ptr::null_mut();
        status(unsafe { GetUnicastIpAddressTable(AF_UNSPEC, &mut addresses) })?;
        let addresses = NativeTable(addresses);
        if addresses.0.is_null() {
            return Err(failure("unicast observation returned a null table"));
        }
        let count = unsafe { (*addresses.0).NumEntries } as usize;
        if count > 1_000_000 {
            return Err(failure("unicast observation exceeds bound"));
        }
        for row in unsafe { std::slice::from_raw_parts((*addresses.0).Table.as_ptr(), count) } {
            if unsafe { row.InterfaceLuid.Value } == owned.luid {
                orphaned.addresses.push(format!(
                    "{}/{}",
                    inet(&row.Address)?,
                    row.OnLinkPrefixLength
                ));
            }
        }
        if let Some(current) = result.iter_mut().find(|item| item.luid == owned.luid) {
            current.routes = orphaned.routes;
            current.addresses = orphaned.addresses;
        } else if !orphaned.routes.is_empty() || !orphaned.addresses.is_empty() {
            result.push(orphaned);
        }
    } else if !previously.is_empty() {
        return Err(failure(
            "adapter observations exist without a durable creation receipt",
        ));
    }
    // Both native address and interface tables can retain nonpresent devices.
    // Classify only after merging them and querying orphan addresses/routes:
    // any remaining network setting still blocks cleanup, even on a phantom.
    let mut index = 0;
    while let Some(item) = result.get(index) {
        let metadata_only =
            item.addresses.is_empty() && item.routes.is_empty() && item.dns_servers.is_empty();
        let phantom = metadata_only
            && presence::is_known_phantom(
                uuid::Uuid::parse_str(&item.guid)
                    .map_err(|_| failure("native interface GUID is invalid"))?,
            )?;
        if phantom {
            result.swap_remove(index);
        } else {
            index += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
