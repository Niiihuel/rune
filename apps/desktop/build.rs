//! `rune.exe`'s icon and version information, for Windows targets only:
//! Explorer, the taskbar, the installer's uninstall entry and the file's
//! properties show them. Other targets build nothing here.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/rune.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("../../assets/icons/rune.ico")
        .set("ProductName", "RUNE")
        .set("FileDescription", "RUNE")
        .set("OriginalFilename", "rune.exe")
        .set("InternalName", "rune");
    // Only the program: tests and examples need no icon.
    if let Err(error) = resource.compile_for("rune") {
        panic!("could not embed rune.exe's icon and version: {error}");
    }
}
