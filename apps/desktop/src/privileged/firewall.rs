use super::protocol::failure;
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, Prefix};
use windows::{
    Win32::{
        Foundation::VARIANT_BOOL,
        NetworkManagement::WindowsFirewall::*,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Ole::{
                IEnumVARIANT, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetLBound,
                SafeArrayGetUBound,
            },
            Variant::*,
        },
    },
    core::{BSTR, Interface},
};

fn com<T>(result: windows::core::Result<T>) -> LanpathResult<T> {
    result.map_err(|error| failure(format!("Windows Firewall: {error}")))
}
fn text(value: BSTR) -> LanpathResult<String> {
    String::from_utf16(&value).map_err(|_| failure("firewall property contains invalid UTF-16"))
}

struct Apartment;
impl Apartment {
    fn enter() -> LanpathResult<Self> {
        com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok())?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Variant(VARIANT);
impl Drop for Variant {
    fn drop(&mut self) {
        let _ = unsafe { VariantClear(&mut self.0) };
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum OptionalText {
    Text(String),
    NotApplicable(i32),
}
fn optional(value: windows::core::Result<BSTR>) -> LanpathResult<OptionalText> {
    match value {
        Ok(value) => Ok(OptionalText::Text(text(value)?)),
        Err(error) if error.code().0 == 0x80070057u32 as i32 => {
            Ok(OptionalText::NotApplicable(error.code().0))
        }
        Err(error) => Err(failure(format!(
            "cannot fingerprint firewall property: {error}"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Interfaces {
    variant: u16,
    lower: i32,
    names: Vec<String>,
}
fn interfaces(value: VARIANT) -> LanpathResult<Interfaces> {
    let value = Variant(value);
    let data = unsafe { &value.0.Anonymous.Anonymous };
    let kind = data.vt.0;
    let mut result = Interfaces {
        variant: kind,
        lower: 0,
        names: Vec::new(),
    };
    if matches!(data.vt, VT_EMPTY | VT_NULL) {
        return Ok(result);
    }
    if kind != VT_ARRAY.0 | VT_VARIANT.0 && kind != VT_ARRAY.0 | VT_BSTR.0 {
        return Err(failure("unsupported firewall interface scope variant"));
    }
    let array = unsafe { data.Anonymous.parray };
    if array.is_null() || unsafe { SafeArrayGetDim(array) } != 1 {
        return Err(failure("invalid firewall interface scope array"));
    }
    let lower = com(unsafe { SafeArrayGetLBound(array, 1) })?;
    let upper = com(unsafe { SafeArrayGetUBound(array, 1) })?;
    if i64::from(upper) - i64::from(lower) > 4095 {
        return Err(failure("firewall interface scope exceeds bound"));
    }
    result.lower = lower;
    for index in lower..=upper {
        let name = if kind == VT_ARRAY.0 | VT_BSTR.0 {
            let mut name = BSTR::new();
            com(unsafe { SafeArrayGetElement(array, &index, (&mut name as *mut BSTR).cast()) })?;
            text(name)?
        } else {
            let mut name = Variant(VARIANT::default());
            com(unsafe {
                SafeArrayGetElement(array, &index, (&mut name.0 as *mut VARIANT).cast())
            })?;
            let data = unsafe { &name.0.Anonymous.Anonymous };
            if data.vt != VT_BSTR {
                return Err(failure("firewall interface is not a string"));
            }
            String::from_utf16(unsafe { &data.Anonymous.bstrVal })
                .map_err(|_| failure("firewall interface contains invalid UTF-16"))?
        };
        result.names.push(name);
    }
    Ok(result)
}

/// Every public INetFwRule/2/3 property participates in compare-before-remove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    name: String,
    application: String,
    description: String,
    service: String,
    protocol: i32,
    local_ports: String,
    remote_ports: String,
    local_addresses: String,
    remote_addresses: String,
    icmp: OptionalText,
    direction: i32,
    interfaces: Interfaces,
    interface_types: String,
    enabled: i16,
    grouping: String,
    profiles: i32,
    edge: i16,
    action: i32,
    edge_options: i32,
    package: OptionalText,
    owner: OptionalText,
    local_users: OptionalText,
    remote_users: OptionalText,
    remote_machines: OptionalText,
    secure_flags: i32,
}
fn snapshot(rule: &INetFwRule) -> LanpathResult<Snapshot> {
    let extended: INetFwRule3 = com(rule.cast())?;
    unsafe {
        Ok(Snapshot {
            name: text(com(rule.Name())?)?,
            application: text(com(rule.ApplicationName())?)?,
            description: text(com(rule.Description())?)?,
            service: text(com(rule.ServiceName())?)?,
            protocol: com(rule.Protocol())?,
            local_ports: text(com(rule.LocalPorts())?)?,
            remote_ports: text(com(rule.RemotePorts())?)?,
            local_addresses: text(com(rule.LocalAddresses())?)?,
            remote_addresses: text(com(rule.RemoteAddresses())?)?,
            icmp: optional(rule.IcmpTypesAndCodes())?,
            direction: com(rule.Direction())?.0,
            interfaces: interfaces(com(rule.Interfaces())?)?,
            interface_types: text(com(rule.InterfaceTypes())?)?,
            enabled: com(rule.Enabled())?.0,
            grouping: text(com(rule.Grouping())?)?,
            profiles: com(rule.Profiles())?,
            edge: com(rule.EdgeTraversal())?.0,
            action: com(rule.Action())?.0,
            edge_options: com(extended.EdgeTraversalOptions())?,
            package: optional(extended.LocalAppPackageId())?,
            owner: optional(extended.LocalUserOwner())?,
            local_users: optional(extended.LocalUserAuthorizedList())?,
            remote_users: optional(extended.RemoteUserAuthorizedList())?,
            remote_machines: optional(extended.RemoteMachineAuthorizedList())?,
            secure_flags: com(extended.SecureFlags())?,
        })
    }
}

pub(super) struct Plan {
    pub rule: INetFwRule,
    pub snapshot: Snapshot,
}
pub(super) struct Firewall {
    rules: INetFwRules,
    _apartment: Apartment,
}
impl Firewall {
    pub fn open() -> LanpathResult<Self> {
        let apartment = Apartment::enter()?;
        let policy: INetFwPolicy2 =
            com(unsafe { CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER) })?;
        Ok(Self {
            rules: com(unsafe { policy.Rules() })?,
            _apartment: apartment,
        })
    }
    fn find(&self, name: &str) -> LanpathResult<Option<Snapshot>> {
        let enumeration: IEnumVARIANT = com(com(unsafe { self.rules._NewEnum() })?.cast())?;
        let mut found = None;
        for _ in 0..65_536 {
            let mut value = Variant(VARIANT::default());
            let mut fetched = 0;
            com(
                unsafe { enumeration.Next(std::slice::from_mut(&mut value.0), &mut fetched) }.ok(),
            )?;
            if fetched == 0 {
                return Ok(found);
            }
            if fetched != 1 {
                return Err(failure("invalid firewall enumeration count"));
            }
            let data = unsafe { &value.0.Anonymous.Anonymous };
            let rule: INetFwRule = match data.vt {
                VT_DISPATCH => com(unsafe { (*data.Anonymous.pdispVal).as_ref() }
                    .ok_or_else(|| failure("null firewall dispatch"))?
                    .cast())?,
                VT_UNKNOWN => com(unsafe { (*data.Anonymous.punkVal).as_ref() }
                    .ok_or_else(|| failure("null firewall interface"))?
                    .cast())?,
                _ => return Err(failure("invalid firewall enumeration variant")),
            };
            if text(com(unsafe { rule.Name() })?)? == name {
                if found.is_some() {
                    return Err(failure(
                        "duplicate owned firewall rule name; no rule removed",
                    ));
                }
                found = Some(snapshot(&rule)?);
            }
        }
        Err(failure("firewall rule enumeration exceeds bound"))
    }
    pub fn prepare(&self, session: uuid::Uuid, image: &Path) -> LanpathResult<Plan> {
        if session.is_nil() {
            return Err(failure("firewall session identity must not be nil"));
        }
        let name = format!("Lanpath GPUI ({session})");
        if self.find(&name)?.is_some() {
            return Err(failure(
                "owned firewall rule name already exists; no rule adopted",
            ));
        }
        let application = image
            .to_str()
            .ok_or_else(|| failure("core image path is not Unicode"))?;
        // Rules.Add rejects Rust's canonical \\?\C:\... representation even
        // though SetApplicationName accepts it. Use the same DOS path alias;
        // the protected stage and its image/ancestor leases remain unchanged.
        let application = match image.components().next() {
            Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::VerbatimDisk(_)) => {
                &application[4..]
            }
            _ => application,
        };
        let rule: INetFwRule =
            com(unsafe { CoCreateInstance(&NetFwRule, None, CLSCTX_INPROC_SERVER) })?;
        unsafe {
            com(rule.SetName(&BSTR::from(name)))?;
            com(rule.SetDescription(&BSTR::from(format!(
                "Temporary com.ybakiame.lanpath.gpui TUN session {session}"
            ))))?;
            com(rule.SetApplicationName(&BSTR::from(application)))?;
            com(rule.SetProtocol(NET_FW_IP_PROTOCOL_TCP.0))?;
            com(rule.SetDirection(NET_FW_RULE_DIR_IN))?;
            com(rule.SetAction(NET_FW_ACTION_ALLOW))?;
            com(rule.SetProfiles(NET_FW_PROFILE2_ALL.0))?;
            com(rule.SetEnabled(VARIANT_BOOL(-1)))?;
            com(rule.SetEdgeTraversal(VARIANT_BOOL(0)))?;
        }
        Ok(Plan {
            snapshot: snapshot(&rule)?,
            rule,
        })
    }
    /// The caller must fsync the complete plan snapshot before calling Add.
    pub fn install(&self, rule: &INetFwRule, expected: &Snapshot) -> LanpathResult<()> {
        if self.find(&expected.name)?.is_some() {
            return Err(failure("firewall rule appeared before installation"));
        }
        com(unsafe { self.rules.Add(rule) })?;
        if self.find(&expected.name)?.as_ref() != Some(expected) {
            return Err(failure(
                "installed firewall rule differs from durable intent; recovery required",
            ));
        }
        Ok(())
    }
    pub fn remove(&self, expected: &Snapshot) -> LanpathResult<()> {
        let Some(current) = self.find(&expected.name)? else {
            return Ok(());
        };
        if current != *expected {
            return Err(failure(
                "owned firewall rule was changed; rule and recovery journal retained",
            ));
        }
        // Firewall COM has no compare-and-delete transaction. Never claim ABA immunity
        // against another concurrently elevated administrator.
        com(unsafe { self.rules.Remove(&BSTR::from(expected.name.as_str())) })?;
        if self.find(&expected.name)?.is_some() {
            return Err(failure("owned firewall rule remains after removal"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "firewall_tests.rs"]
mod tests;
