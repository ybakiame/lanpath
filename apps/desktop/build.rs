//! Embed the shared Windows icon for the window, taskbar and Explorer.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../assets/icons/icon.ico");
    println!("cargo:rerun-if-changed=../../VERSION");
    let version = std::env::var("CARGO_PKG_VERSION")?;
    if version != include_str!("../../VERSION").trim() {
        return Err(
            "Product version mismatch; run python3 scripts/version.py --set VERSION".into(),
        );
    }
    if std::env::var("CARGO_CFG_TARGET_OS")? == "windows" {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("../../assets/icons/icon.ico");
        resource.set("ProductName", "Lanpath");
        resource.set("FileDescription", "Lanpath NSH Desktop");
        resource.set("OriginalFilename", "lanpath-desktop-nsh.exe");
        resource.set("CompanyName", "Lanpath Contributors");
        resource.set("ProductVersion", &version);
        resource.set("FileVersion", &version);
        let fields: Vec<&str> = version.split(['.', '-']).collect();
        if fields.len() != 5 || fields[3] != "rc" {
            return Err("Expected product version 0.x.y-rc.N".into());
        }
        let mut numeric = 0_u64;
        for index in [0, 1, 2, 4] {
            numeric = (numeric << 16) | u64::from(fields[index].parse::<u16>()?);
        }
        resource.set_version_info(winres::VersionInfo::PRODUCTVERSION, numeric);
        resource.set_version_info(winres::VersionInfo::FILEVERSION, numeric);
        resource.set_version_info(winres::VersionInfo::FILEFLAGS, 0x2); // VS_FF_PRERELEASE
        resource.compile()?;
    }
    Ok(())
}
