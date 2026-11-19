//! Embed the shared Windows icon for the window, taskbar and Explorer.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../assets/icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS")? == "windows" {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("../../assets/icons/icon.ico");
        resource.set("ProductName", "Lanpath");
        resource.set("FileDescription", "Lanpath NSH Desktop");
        resource.set("OriginalFilename", "lanpath-desktop-nsh.exe");
        resource.set("CompanyName", "Lanpath Contributors");
        resource.compile()?;
    }
    Ok(())
}
