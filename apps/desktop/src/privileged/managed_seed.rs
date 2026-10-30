use super::{protocol::failure, staging};
use lanpath_client::{core_management::CoreManagementService, cores::seed_bundled_core};
use lanpath_common::LanpathResult;
use std::path::Path;

/// Ordinary-user installation only. The runtime holds its stopped-core lease;
/// the existing shared installer and selector retain their inventory contract.
pub(crate) fn install_bundled_core(root: &Path) -> LanpathResult<()> {
    let service = CoreManagementService::experimental(root.to_path_buf());
    // Reject unreadable configuration/inventory before creating any installation.
    let local = service.list_local()?;
    let trusted = staging::trusted_bundle()?;
    if let Some(core) = local
        .iter()
        .find(|core| core.version() == crate::product::CORE_MANAGED_VERSION)
    {
        let _directory = crate::windows_access::private_root(
            core.path()
                .parent()
                .ok_or_else(|| failure("managed core has no version directory"))?,
        )?;
        verify_core(&trusted, core.path())?;
        return service.select(core.path());
    }
    let scratch = tempfile::Builder::new()
        .prefix(".managed-seed-")
        .tempdir_in(root)?;
    let _scratch = crate::windows_access::private_root(scratch.path())?;
    let seed = seed_bundled_core(scratch.path(), &trusted.root.join("resources"))?
        .ok_or_else(|| failure("bundled managed core could not be installed and version-verified; the saved selection is unchanged"))?;
    if seed.version != crate::product::CORE_MANAGED_VERSION {
        return Err(failure(
            "bundled managed core version differs from this application",
        ));
    }
    let relative = seed
        .path
        .strip_prefix(scratch.path())
        .map_err(|_| failure("seed installer returned an external path"))?;
    let target = root.join(relative);
    let target_directory = target
        .parent()
        .ok_or_else(|| failure("managed core has no version directory"))?;
    let parent = target_directory
        .parent()
        .ok_or_else(|| failure("managed core has no inventory parent"))?;
    let _parent = crate::windows_access::private_root(parent)?;
    verify_core(&trusted, &seed.path)?;
    if target_directory.try_exists()? {
        // Never replace an existing version, even when it is currently inactive.
        let _existing = crate::windows_access::private_root(target_directory)?;
        verify_core(&trusted, &target)?;
    } else {
        std::fs::rename(
            seed.path
                .parent()
                .ok_or_else(|| failure("seed core has no version directory"))?,
            target_directory,
        )?;
    }
    service.select(&target)
}

fn verify_core(trusted: &staging::TrustedBundle, binary: &Path) -> LanpathResult<()> {
    trusted.verify_file(Path::new("resources/seed/sing-box.exe"), binary)?;
    trusted.verify_file(
        Path::new("resources/seed/wintun.dll"),
        &binary.with_file_name("wintun.dll"),
    )
}
