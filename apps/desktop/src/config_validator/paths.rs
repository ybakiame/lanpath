//! 资源路径收敛（限制在数据根内）：配置引用的读/写文件路径必须落在本应用
//! 数据根内，拒绝 `..`、Windows 设备/ADS 形式、`$` 环境变量展开与符号链接/
//! junction/硬链接逃逸。这是配置收敛，不防替换文件的恶意进程。

use std::path::{Component, Path, PathBuf};

use lanpath_common::LanpathResult;

use super::denied;

#[cfg(windows)]
fn relative_to_root<'a>(candidate: &'a Path, root: &Path) -> Option<&'a Path> {
    use std::path::Prefix;
    let mut remaining = candidate.components();
    for expected in root.components() {
        match (remaining.next(), expected) {
            (Some(Component::Prefix(actual)), Component::Prefix(expected)) => {
                let equal = match (actual.kind(), expected.kind()) {
                    (
                        Prefix::Disk(a) | Prefix::VerbatimDisk(a),
                        Prefix::Disk(b) | Prefix::VerbatimDisk(b),
                    ) => a.eq_ignore_ascii_case(&b),
                    (
                        Prefix::UNC(a, b) | Prefix::VerbatimUNC(a, b),
                        Prefix::UNC(c, d) | Prefix::VerbatimUNC(c, d),
                    ) => a == c && b == d,
                    (a, b) => a == b,
                };
                if !equal {
                    return None;
                }
            }
            (Some(actual), expected) if actual == expected => {}
            _ => return None,
        }
    }
    Some(remaining.as_path())
}

/// 把配置里的资源路径收敛到数据根内：绝对路径必须位于根下；相对路径按
/// 根相对解析，拒绝 `..`、Windows 设备/ADS 形式、`$` 环境变量展开；逐级
/// 检查符号链接/junction/硬链接逃逸。写路径还要求父目录位于指定子目录
/// （`logs/`、`cache/`）内。缺失的叶子允许（核心自行创建），缺失的父目录
/// 拒绝。
pub(crate) fn confined(root: &Path, text: &str, location: &str) -> LanpathResult<PathBuf> {
    if text.is_empty() || text.contains('\0') {
        return Err(denied(location, "需要非空文件路径"));
    }
    if text.contains('$') {
        return Err(denied(location, "不支持环境变量资源路径"));
    }
    let canonical_root = std::fs::canonicalize(root)?;
    let candidate = Path::new(text);
    let relative = if candidate.is_absolute() {
        #[cfg(windows)]
        let stripped = relative_to_root(candidate, &canonical_root);
        #[cfg(not(windows))]
        let stripped = candidate.strip_prefix(&canonical_root).ok();
        stripped
            .map(Path::to_path_buf)
            .ok_or_else(|| denied(location, "绝对资源路径必须位于数据根内"))?
    } else {
        if text.contains(':') || text.contains('\\') {
            return Err(denied(location, "相对资源路径使用 '/' 分隔且不得含冒号"));
        }
        candidate.to_path_buf()
    };
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(denied(location, "资源路径不得包含根、'.' 或 '..' 组件"));
    }
    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        let stem = name.split('.').next().unwrap_or_default();
        if name.is_empty()
            || name.ends_with(['.', ' '])
            || name.chars().any(|c| c.is_control() || "<>|?*:".contains(c))
            || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
                .iter()
                .any(|reserved| stem.eq_ignore_ascii_case(reserved))
        {
            return Err(denied(location, "不支持或 Windows 特殊资源路径组件"));
        }
    }
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
                    return Err(denied(location, "不支持符号链接/junction 资源路径"));
                }
                if !leaf && !metadata.is_dir() {
                    return Err(denied(location, "资源父路径不是目录"));
                }
                if leaf && !metadata.is_file() {
                    return Err(denied(location, "资源必须是普通文件"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if metadata.is_file() && metadata.nlink() != 1 {
                        return Err(denied(location, "不支持硬链接资源"));
                    }
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err(denied(location, "不支持 reparse point 资源"));
                    }
                    if metadata.is_file()
                        && crate::windows_file::link_count(&std::fs::File::open(&path)?)? != 1
                    {
                        return Err(denied(location, "不支持硬链接资源"));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !leaf {
                    return Err(denied(location, "资源父目录必须已存在"));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

/// 写路径额外要求父目录位于根的指定子目录（如 `logs/`、`runtime/`、`.cache/`）内。
pub(crate) fn write_confined(
    root: &Path,
    text: &str,
    location: &str,
    directory: &str,
) -> LanpathResult<PathBuf> {
    write_confined_any(root, text, location, &[directory])
}

/// 写路径额外要求父目录位于根的允许子目录集合内。
pub(crate) fn write_confined_any(
    root: &Path,
    text: &str,
    location: &str,
    directories: &[&str],
) -> LanpathResult<PathBuf> {
    let path = confined(root, text, location)?;
    let canonical_root = std::fs::canonicalize(root)?;
    let relative = path
        .strip_prefix(&canonical_root)
        .map_err(|_| denied(location, "输出路径越出数据根"))?;
    if relative
        .parent()
        .is_none_or(|parent| !directories.iter().any(|dir| parent.starts_with(dir)))
    {
        let joined = directories
            .iter()
            .map(|dir| format!("{dir}/"))
            .collect::<Vec<_>>()
            .join(" 或 ");
        return Err(denied(
            location,
            &format!("输出必须位于根内 {joined} 目录（先创建该目录）"),
        ));
    }
    Ok(path)
}
