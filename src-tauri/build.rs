fn main() {
    #[cfg(target_os = "windows")]
    build_windows_titlebar();

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

#[cfg(target_os = "windows")]
fn build_windows_titlebar() {
    use std::{
        env, fs,
        path::{Path, PathBuf},
        process::Command,
    };

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let native = manifest.join("native/windows");
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("aarch64") => "ARM64",
        other => panic!("unsupported Windows architecture for title bar bridge: {other:?}"),
    };
    let configuration = if env::var_os("DEBUG").as_deref() == Some(std::ffi::OsStr::new("true")) {
        "Debug"
    } else {
        "Release"
    };
    let stage = native.join("runtime").join(arch.to_ascii_lowercase());
    let cargo_out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let native_out = cargo_out.join("windows-titlebar");
    let native_obj = cargo_out.join("windows-titlebar-obj");
    fs::create_dir_all(&stage).expect("create Windows title bar runtime directory");

    let vswhere =
        Path::new(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    let msbuild = if vswhere.exists() {
        let output = Command::new(vswhere)
            .args([
                "-latest",
                "-products",
                "*",
                "-requires",
                "Microsoft.Component.MSBuild",
                "-find",
                r"MSBuild\**\Bin\MSBuild.exe",
            ])
            .output()
            .expect("run vswhere");
        let output = String::from_utf8(output.stdout).expect("vswhere output is UTF-8");
        PathBuf::from(
            output
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .expect("vswhere found no MSBuild"),
        )
    } else {
        PathBuf::from("MSBuild.exe")
    };
    let status = Command::new(msbuild)
        .arg(native.join("Verenu.WindowsChrome.vcxproj"))
        .args(["/restore", "/m", "/v:minimal"])
        .arg(format!("/p:Configuration={configuration}"))
        .arg(format!("/p:Platform={arch}"))
        .arg(format!("/p:OutDir={}\\", native_out.display()))
        .arg(format!("/p:IntDir={}\\", native_obj.display()))
        .status()
        .expect("run MSBuild for Windows title bar bridge");
    assert!(status.success(), "Windows title bar bridge failed to build");

    fn copy_changed(source: &Path, destination: &Path) {
        let unchanged = fs::read(source)
            .ok()
            .zip(fs::read(destination).ok())
            .is_some_and(|(a, b)| a == b);
        if !unchanged {
            fs::copy(source, destination).unwrap_or_else(|error| {
                panic!(
                    "copy {} to {}: {error}",
                    source.display(),
                    destination.display()
                )
            });
        }
    }

    for name in [
        "Verenu.WindowsChrome.dll",
        "Verenu.WindowsChrome.pri",
        "Microsoft.WindowsAppRuntime.Bootstrap.dll",
    ] {
        let source = native_out.join(name);
        if source.exists() {
            copy_changed(&source, &stage.join(name));
        }
    }

    // The Tauri executable is the deployment host. Stage the pinned Windows App SDK
    // framework payload beside it so packaged builds never rely on a developer runtime.
    let packages = env::var_os("NUGET_PACKAGES")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("USERPROFILE").expect("USERPROFILE is set"))
                .join(".nuget/packages")
        });
    for (package, version) in [
        ("microsoft.windowsappsdk.foundation", "1.8.260505001"),
        (
            "microsoft.windowsappsdk.interactiveexperiences",
            "1.8.260708001",
        ),
    ] {
        let payload = packages
            .join(package)
            .join(version)
            .join("runtimes-framework")
            .join(format!("win-{}", arch.to_ascii_lowercase()))
            .join("native");
        for entry in fs::read_dir(&payload).unwrap_or_else(|error| {
            panic!(
                "read Windows App SDK payload {}: {error}",
                payload.display()
            )
        }) {
            let entry = entry.expect("read Windows App SDK payload entry");
            if entry.file_type().expect("read payload type").is_file() {
                copy_changed(&entry.path(), &stage.join(entry.file_name()));
            }
        }
    }

    let profile = env::var("PROFILE").expect("Cargo profile");
    let target_profile = cargo_out
        .ancestors()
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name == profile.as_str())
        })
        .expect("Cargo profile directory in OUT_DIR")
        .to_path_buf();
    fs::create_dir_all(&target_profile).expect("create Cargo profile directory");
    for entry in fs::read_dir(&stage).expect("read staged title bar runtime") {
        let entry = entry.expect("read staged runtime entry");
        if entry
            .file_type()
            .expect("read staged runtime type")
            .is_file()
        {
            copy_changed(&entry.path(), &target_profile.join(entry.file_name()));
        }
    }

    println!("cargo:rerun-if-changed=native/windows/extended_titlebar.cpp");
    println!("cargo:rerun-if-changed=native/windows/extended_titlebar.h");
    println!("cargo:rerun-if-changed=native/windows/Verenu.WindowsChrome.vcxproj");
}

