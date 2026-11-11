use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=product.json");
    println!("cargo:rerun-if-changed=../client/src-tauri/icons/icon.ico");
    println!("cargo:rerun-if-changed=app.manifest");
    let metadata: serde_json::Value = serde_json::from_str(&fs::read_to_string("product.json")?)?;
    let fields = [
        ("DISPLAY_NAME", "display_name"),
        ("APP_ID", "app_id"),
        ("DATA_DIR_NAME", "data_dir_name"),
        ("BINARY_NAME", "binary_name"),
        ("PORTABLE_MARKER", "portable_marker"),
        ("UPDATE_CHANNEL", "update_channel"),
        ("UPDATE_ENDPOINT", "update_endpoint"),
        ("UPDATE_PUBLIC_KEY", "update_public_key"),
        ("REPOSITORY", "repository"),
    ];
    let mut generated = String::new();
    for (constant, key) in fields {
        let value = metadata[key]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("product.json requires nonempty {key}"))?;
        generated.push_str(&format!("pub(crate) const {constant}: &str = {value:?};\n"));
    }
    let tun_supported = metadata["tun_supported"]
        .as_bool()
        .ok_or("product.json requires boolean tun_supported")?;
    generated.push_str(&format!(
        "pub(crate) const TUN_SUPPORTED: bool = {tun_supported};\n"
    ));
    let bundle_files = metadata["bundle_files"]
        .as_array()
        .ok_or("missing bundle_files")?;
    let mut paths = std::collections::HashSet::new();
    generated.push_str("pub(crate) const TRUSTED_BUNDLE_FILES: &[&str] = &[\n");
    for entry in bundle_files {
        let path = entry.as_str().ok_or("bundle_files must contain strings")?;
        if path.is_empty()
            || path.contains(['\\', ':'])
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || !paths.insert(path.to_ascii_lowercase())
        {
            return Err("invalid or duplicate bundle_files entry".into());
        }
        generated.push_str(&format!("    {path:?},\n"));
    }
    if !paths.contains(
        metadata["binary_name"]
            .as_str()
            .ok_or("missing binary_name")?,
    ) {
        return Err("bundle_files must include binary_name".into());
    }
    generated.push_str("];\n");
    let version = env::var("CARGO_PKG_VERSION")?;
    generated.push_str(&format!("pub(crate) const VERSION: &str = {version:?};\n"));
    let managed_digest = generate_managed_constants(&mut generated)?;
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("product_constants.rs"),
        generated,
    )?;
    if env::var("CARGO_CFG_TARGET_OS")? == "windows" {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("../client/src-tauri/icons/icon.ico");
        resource.set("LanpathManagedCoreSource", &managed_digest);
        // gpui-pre embeds manifest resource #1. Product packaging replaces that
        // single resource before hashing/signing; embedding another one here
        // duplicates #1 and fails MSVC's resource linker.
        for (name, value) in [
            (
                "ProductName",
                metadata["display_name"].as_str().ok_or("missing name")?,
            ),
            (
                "FileDescription",
                metadata["display_name"].as_str().ok_or("missing name")?,
            ),
            (
                "OriginalFilename",
                metadata["binary_name"].as_str().ok_or("missing binary")?,
            ),
            ("CompanyName", "Lanpath Contributors"),
            ("LegalCopyright", "Lanpath and ProxyPanel Contributors"),
            ("ProductVersion", version.as_str()),
            ("FileVersion", version.as_str()),
        ] {
            resource.set(name, value);
        }
        resource.compile()?;
    }
    Ok(())
}

fn generate_managed_constants(
    generated: &mut String,
) -> Result<String, Box<dyn std::error::Error>> {
    let root = PathBuf::from("managed-core");
    let source_path = root.join("source.json");
    let source_metadata = fs::symlink_metadata(&source_path)?;
    if !source_metadata.is_file()
        || source_metadata.file_type().is_symlink()
        || source_metadata.len() > 128 * 1024
    {
        return Err("managed source identity must be a bounded ordinary file".into());
    }
    let source_bytes = fs::read(source_path)?;
    let source: serde_json::Value = serde_json::from_slice(&source_bytes)?;
    if source["schema"].as_u64() != Some(1) || source["managed_protocol"].as_u64() != Some(1) {
        return Err("unsupported managed core source/protocol schema".into());
    }
    let mut files = vec!["source.json"];
    for entry in source["managed_files"]
        .as_array()
        .ok_or("missing managed_files")?
    {
        let name = entry.as_str().ok_or("invalid managed file")?;
        let mut parts = name.split('/');
        let directory = parts.next().ok_or("missing managed source directory")?;
        let file = parts.next().ok_or("missing managed source file")?;
        if parts.next().is_some()
            || !["core", "tun", "patches"].contains(&directory)
            || !file
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b'_' | b'.'))
            || !(name.ends_with(".go") || name.ends_with(".patch"))
        {
            return Err("invalid managed source input path".into());
        }
        files.push(name);
    }
    files.sort_unstable();
    if files.len() > 32 || files.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("invalid managed source inventory".into());
    }
    let mut hash = Sha256::new();
    for name in files {
        let path = root.join(name);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 128 * 1024 {
            return Err("managed inputs must be bounded ordinary files".into());
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = if name == "source.json" {
            std::borrow::Cow::Borrowed(source_bytes.as_slice())
        } else {
            std::borrow::Cow::Owned(fs::read(path)?)
        };
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(bytes.as_ref());
        hash.update([0]);
    }
    for (constant, field) in [
        ("CORE_MANAGED_VERSION", "version"),
        ("CORE_UPSTREAM_VERSION", "upstream_version"),
        ("CORE_GO_VERSION", "go_version"),
    ] {
        let value = source[field]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or("missing managed source identity")?;
        generated.push_str(&format!(
            "#[cfg(windows)]\npub(crate) const {constant}: &str = {value:?};\n"
        ));
    }
    let digest = format!("{:x}", hash.finalize());
    generated.push_str(&format!(
        "#[cfg(windows)]\npub(crate) const CORE_MANAGED_SOURCE_DIGEST: &str = {digest:?};\n"
    ));
    generated.push_str("#[cfg(windows)]\npub(crate) const CORE_MANAGED_PROTOCOL: u32 = 1;\n");
    Ok(digest)
}
