use super::{protocol::failure, security};
use lanpath_common::LanpathResult;
use std::{marker::PhantomData, os::windows::fs::MetadataExt, path::Path, rc::Rc};
use windows::{
    Win32::{
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::Shell::*,
    },
    core::PCWSTR,
};

fn com<T>(site: &'static str, result: windows::core::Result<T>) -> LanpathResult<T> {
    result.map_err(|error| failure(format!("Windows helper staging [{site}]: {error}")))
}
struct Apartment(PhantomData<Rc<()>>);
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

/// Shell file operations require an STA even when invoked from an MTA runtime.
/// Never change the caller's COM mode or move live COM interfaces across threads.
#[cfg(test)]
pub(super) fn run<T: Send>(
    phase: &'static str,
    work: impl FnOnce(&Operation) -> LanpathResult<T> + Send,
) -> LanpathResult<T> {
    session(|operation| operation.batch(phase, work))
}

pub(super) fn session<T: Send>(
    work: impl FnOnce(&Operation) -> LanpathResult<T> + Send,
) -> LanpathResult<T> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("lanpath-code-stage".into())
            .spawn_scoped(scope, || {
                let operation = Operation::new()?;
                work(&operation)
            })?
            .join()
            .map_err(|_| failure("Windows code-staging worker panicked"))?
    })
}

/// Ordinary-user access to Windows' own elevation broker. Callers bind all
/// paths to the authenticated code inventory and retain source/ancestor leases.
/// Field order releases the COM object before uninitializing its STA.
/// One STA and Shell object span all phases; validation and directory leases
/// remain between explicit batches. Failed batches end the staging session.
pub(super) struct Operation {
    operation: IFileOperation,
    _apartment: Apartment,
}
impl Operation {
    fn new() -> LanpathResult<Self> {
        com(
            "CoInitializeEx",
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok(),
        )?;
        let apartment = Apartment(PhantomData);
        let operation: IFileOperation = com("CoCreateInstance", unsafe {
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)
        })?;
        com("SetOperationFlags", unsafe {
            operation.SetOperationFlags(default_flags())
        })?;
        Ok(Self {
            operation,
            _apartment: apartment,
        })
    }
    pub fn batch<T>(
        &self,
        phase: &'static str,
        work: impl FnOnce(&Self) -> LanpathResult<T>,
    ) -> LanpathResult<T> {
        // Delete-only confirmation flags must not leak into later copy/promotion.
        com("batch.SetOperationFlags", unsafe {
            self.operation.SetOperationFlags(default_flags())
        })?;
        let result = work(self)?;
        self.perform(phase)?;
        Ok(result)
    }
    fn perform(&self, site: &'static str) -> LanpathResult<()> {
        com(site, unsafe { self.operation.PerformOperations() })?;
        if com("GetAnyOperationsAborted", unsafe {
            self.operation.GetAnyOperationsAborted()
        })?
        .as_bool()
        {
            return Err(failure(
                "Windows helper staging was cancelled or incomplete; no application was elevated",
            ));
        }
        Ok(())
    }
    pub fn queue_create_directory(&self, path: &Path) -> LanpathResult<()> {
        let parent = item(parent(path)?)?;
        let name = name(path)?;
        com("NewItem", unsafe {
            self.operation.NewItem(
                &parent,
                windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY,
                PCWSTR(name.as_ptr()),
                None,
                None,
            )
        })
    }
    pub fn queue_copy_file(&self, source: &Path, target: &Path) -> LanpathResult<()> {
        regular(source)?;
        absent(target)?;
        let source = item(source)?;
        let parent = item(parent(target)?)?;
        let name = name(target)?;
        com("CopyItem", unsafe {
            self.operation
                .CopyItem(&source, &parent, PCWSTR(name.as_ptr()), None)
        })
    }
    pub fn queue_rename_file(&self, source: &Path, target: &Path) -> LanpathResult<()> {
        regular(source)?;
        absent(target)?;
        if parent(source)? != parent(target)? {
            return Err(failure("code promotion cannot move between directories"));
        }
        let source = item(source)?;
        let name = name(target)?;
        com("RenameItem", unsafe {
            self.operation
                .RenameItem(&source, PCWSTR(name.as_ptr()), None)
        })
    }
    pub fn queue_delete_files<P: AsRef<Path>>(
        &self,
        paths: impl IntoIterator<Item = P>,
    ) -> LanpathResult<()> {
        // Only caller-classified disposable leaves skip ordinary confirmation;
        // copy/rename collisions and UAC consent stay interactive.
        com("delete.SetOperationFlags", unsafe {
            self.operation
                .SetOperationFlags(default_flags() | FOF_NOCONFIRMATION)
        })?;
        for path in paths {
            let path = path.as_ref();
            regular(path)?;
            let source = item(path)?;
            com("DeleteItem", unsafe {
                self.operation.DeleteItem(&source, None)
            })?;
        }
        Ok(())
    }
}
fn default_flags() -> FILEOPERATION_FLAGS {
    FOF_NOCOPYSECURITYATTRIBS
        | FOF_NOCONFIRMMKDIR
        | FOF_NORECURSION
        | FOF_NO_CONNECTED_ELEMENTS
        | FOF_SILENT
        | FOF_NOERRORUI
        | FOFX_EARLYFAILURE
        | FOFX_NOCOPYHOOKS
        | FOFX_SHOWELEVATIONPROMPT
        | FOFX_REQUIREELEVATION
}
fn item(path: &Path) -> LanpathResult<IShellItem> {
    // Bundle inventory paths use '/', which Win32 accepts but Shell parsing
    // rejects. Normalize separators in the existing UTF-16 buffer; do not
    // canonicalize again or release the caller's identity-bound path leases.
    let mut path = security::wide(path);
    for unit in &mut path {
        if *unit == u16::from(b'/') {
            *unit = u16::from(b'\\');
        }
    }
    com("SHCreateItemFromParsingName", unsafe {
        SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None)
    })
}
fn parent(path: &Path) -> LanpathResult<&Path> {
    path.parent()
        .ok_or_else(|| failure("code payload has no parent"))
}
fn name(path: &Path) -> LanpathResult<Vec<u16>> {
    Ok(security::wide(
        path.file_name()
            .ok_or_else(|| failure("code payload has no name"))?,
    ))
}
fn absent(path: &Path) -> LanpathResult<()> {
    if path.try_exists()? {
        return Err(failure(
            "code target already exists; no existing payload was overwritten",
        ));
    }
    Ok(())
}
fn regular(path: &Path) -> LanpathResult<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
    {
        return Err(failure(
            "code operation requires an ordinary non-reparse file",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "file_operation_tests.rs"]
mod tests;
