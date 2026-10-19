use std::fs::File;
use std::os::windows::io::AsRawHandle;

pub(crate) fn link_count(file: &File) -> std::io::Result<u32> {
    // BY_HANDLE_FILE_INFORMATION consists exclusively of DWORDs; each FILETIME
    // is two DWORDs. Keep its exact documented C layout.
    #[repr(C)]
    struct Information {
        attributes: u32,
        creation: [u32; 2],
        access: [u32; 2],
        write: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            file: *mut std::ffi::c_void,
            information: *mut Information,
        ) -> i32;
    }
    let mut information = std::mem::MaybeUninit::<Information>::uninit();
    // SAFETY: file owns a live handle, and the output is correctly sized,
    // aligned, writable storage for the Win32 structure.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a successful call initialized every structure field.
    Ok(unsafe { information.assume_init() }.links)
}
