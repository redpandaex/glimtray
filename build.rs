fn main() {
    slint_build::compile("ui/app-window.slint").expect("Failed to compile the Slint UI");

    println!("cargo:rerun-if-changed=assets/icons/glimtray.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icons/glimtray.ico")
            .set("ProductName", "GlimTray")
            .set("FileDescription", "GlimTray - Monitor brightness control")
            .set("OriginalFilename", "glimtray.exe")
            .compile()
            .expect("Failed to compile the Windows icon resource");
    }
}
