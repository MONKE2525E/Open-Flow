# Windows extended title bar bridge

Verenu keeps Tauri's decorated top-level HWND and passes that HWND to this small C++/WinRT DLL. The bridge bootstraps the unpackaged Windows App SDK host, resolves the HWND through `GetWindowIdFromWindow`, and enables `AppWindowTitleBar.ExtendsContentIntoTitleBar`. Windows continues to create and own the caption-control HWND and non-client hit testing.

`build.rs` restores and builds the project only on Windows. Release builds stage the pinned Windows App SDK 1.8 framework payload from its NuGet component packages into `native/windows/runtime/<architecture>`. Tauri maps that directory to the installer resource root, so the bridge, runtime DLLs, and PRI files are installed beside `Verenu.exe`. This is an unpackaged self-contained payload and does not depend on a developer runtime being present.

The C ABI is intentionally narrow: enable the extended title bar, refresh theme and metrics, and read metrics. All geometry returned by Windows is in physical pixels. Rust converts it to CSS pixels before emitting it to Svelte.
