fn main() {
    println!("cargo:rerun-if-changed=windows/sortdeck.manifest");

    let windows_attrs = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!("windows/sortdeck.manifest"));
    let attrs = tauri_build::Attributes::new().windows_attributes(windows_attrs);

    if let Err(err) = tauri_build::try_build(attrs) {
        eprintln!("failed to run tauri-build: {err}");
        std::process::exit(1);
    }
}
