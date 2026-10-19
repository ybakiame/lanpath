use std::path::{Component, Path, PathBuf};

use lanpath_common::{LanpathError, LanpathResult};
use serde_json::Value;

pub(super) fn denied(location: &str, reason: &str) -> LanpathError {
    LanpathError::Config(format!("{location}: {reason}"))
}

/// Check each existing component, including Windows junctions (whose canonical
/// target differs even when their file type is not reported as a symlink).
/// Missing leaves are permitted for output files, never missing parents.
/// This is configuration confinement, not protection against a hostile process
/// with permission to replace files after validation.
pub(super) fn confined(root: &Path, text: &str, location: &str) -> LanpathResult<PathBuf> {
    if text.is_empty() || text.contains('\0') {
        return Err(denied(location, "expected a nonempty file path"));
    }
    if text.contains('$') {
        return Err(denied(
            location,
            "environment-variable resource paths are unsupported",
        ));
    }
    // A colon also rules out Windows alternate data streams and device paths.
    // Absolute paths are intentionally unsupported: portable resources are root-relative.
    if text.contains(':') || text.contains('\\') {
        return Err(denied(
            location,
            "use a root-relative path with '/' separators",
        ));
    }
    let relative = Path::new(text);
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(denied(
            location,
            "absolute, '.' and '..' resource paths are unsupported",
        ));
    }
    for name in text.split('/') {
        let stem = name.split('.').next().unwrap_or_default();
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.ends_with(['.', ' '])
            || name.chars().any(|c| c.is_control() || "<>|?*".contains(c))
            || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
                .iter()
                .any(|reserved| stem.eq_ignore_ascii_case(reserved))
            || (stem.get(..3).is_some_and(|prefix| {
                prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
            }) && matches!(
                stem.get(3..),
                Some("0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
            ))
        {
            return Err(denied(
                location,
                "unsupported or Windows-special resource path component",
            ));
        }
    }
    let canonical_root = std::fs::canonicalize(root)?;
    let mut path = canonical_root.clone();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let leaf = components.peek().is_none();
        path.push(component.as_os_str());
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                let canonical = std::fs::canonicalize(&path)?;
                if metadata.file_type().is_symlink()
                    || !canonical.starts_with(&canonical_root)
                    || canonical != path
                {
                    return Err(denied(
                        location,
                        "symlink/junction resource paths are unsupported",
                    ));
                }
                if !leaf && !metadata.is_dir() {
                    return Err(denied(location, "a resource parent is not a directory"));
                }
                if leaf && !metadata.is_file() {
                    return Err(denied(location, "resource must be a regular file"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if metadata.is_file() && metadata.nlink() != 1 {
                        return Err(denied(location, "hard-linked resources are unsupported"));
                    }
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err(denied(location, "reparse-point resources are unsupported"));
                    }
                    if metadata.is_file()
                        && crate::windows_file::link_count(&std::fs::File::open(&path)?)? != 1
                    {
                        return Err(denied(location, "hard-linked resources are unsupported"));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !leaf {
                    return Err(denied(
                        location,
                        "resource parent directory must already exist",
                    ));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

pub(super) fn read_paths(root: &Path, value: &Value, location: &str) -> LanpathResult<()> {
    if let Some(paths) = value.as_array() {
        for (index, path) in paths.iter().enumerate() {
            read_paths(root, path, &format!("{location}[{index}]"))?;
        }
        return Ok(());
    }
    let text = value
        .as_str()
        .ok_or_else(|| denied(location, "expected a file path or array of paths"))?;
    let path = confined(root, text, location)?;
    if !path.is_file() {
        return Err(denied(location, "resource file does not exist"));
    }
    Ok(())
}

/// Restrict writes to dedicated subdirectories, so a valid core configuration
/// cannot truncate configuration.json, the engine's sing-box.json, locks, the
/// selected executable, or application databases in the data-root itself.
pub(super) fn write_path(
    root: &Path,
    value: &Value,
    location: &str,
    directory: &str,
) -> LanpathResult<()> {
    let text = value
        .as_str()
        .ok_or_else(|| denied(location, "expected a file path"))?;
    let relative = Path::new(text);
    if relative.parent().is_none_or(|p| !p.starts_with(directory)) {
        return Err(denied(
            location,
            &format!("output must be inside the root/{directory}/ directory (create it first)"),
        ));
    }
    confined(root, text, location)?;
    Ok(())
}

pub(super) fn source_path(root: &Path) -> LanpathResult<PathBuf> {
    confined(root, "configuration.json", "configuration.json")
}
