//! Embeds the app icon and version information in `tortuga.exe` on Windows (R-NF-4),
//! so Explorer, the taskbar and the Start menu show the turtle.

fn main() {
    println!("cargo::rerun-if-changed=icons/tortuga.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("icons/tortuga.ico");
    resource.set("ProductName", "Tortuga");
    resource.set("FileDescription", "Tortuga API client");
    if let Err(e) = resource.compile() {
        // Without the Windows resource compiler the app still builds, just without its icon.
        println!("cargo::warning=could not embed the Windows icon: {e}");
    }
}
