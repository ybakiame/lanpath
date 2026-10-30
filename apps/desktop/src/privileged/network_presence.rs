use lanpath_common::LanpathResult;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_LOCATE_DEVNODE_NORMAL, CM_LOCATE_DEVNODE_PHANTOM, CM_Locate_DevNodeW, CR_NO_SUCH_DEVNODE,
    CR_SUCCESS,
};

use super::failure;

/// Wintun retains a nonpresent PnP node and sometimes an interface row after
/// removal. Only an explicit NORMAL-missing / PHANTOM-present pair proves this
/// row is metadata rather than an active adapter. Unknown nodes remain evidence.
/// https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_locate_devnodew
pub(super) fn is_known_phantom(guid: uuid::Uuid) -> LanpathResult<bool> {
    let mut encoded = [0u8; 36];
    let guid = guid.hyphenated().encode_upper(&mut encoded);
    let mut instance = [0u16; 50];
    for (unit, byte) in instance[..12].iter_mut().zip(b"SWD\\WINTUN\\{") {
        *unit = u16::from(*byte);
    }
    for (unit, byte) in instance[12..48].iter_mut().zip(guid.as_bytes()) {
        *unit = u16::from(*byte);
    }
    instance[48] = u16::from(b'}');
    let mut device = 0;
    let present =
        unsafe { CM_Locate_DevNodeW(&mut device, instance.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) };
    if present == CR_SUCCESS {
        return Ok(false);
    }
    if present != CR_NO_SUCH_DEVNODE {
        return Err(failure(format!(
            "native device presence query failed: {present:#x}"
        )));
    }
    let phantom =
        unsafe { CM_Locate_DevNodeW(&mut device, instance.as_ptr(), CM_LOCATE_DEVNODE_PHANTOM) };
    match phantom {
        CR_SUCCESS => Ok(true),
        CR_NO_SUCH_DEVNODE => Ok(false),
        code => Err(failure(format!("native phantom query failed: {code:#x}"))),
    }
}
