fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=src/system/macos_ax_text_marker.m");
        cc::Build::new()
            .file("src/system/macos_ax_text_marker.m")
            .flag("-fobjc-arc")
            .compile("verenu_macos_ax_text_marker");
    }

    println!("cargo:rerun-if-changed=Info.plist");
    println!("cargo:rerun-if-changed=icons/icon.png");
    println!("cargo:rerun-if-changed=icons/icon.icns");
    tauri_build::build()
}
