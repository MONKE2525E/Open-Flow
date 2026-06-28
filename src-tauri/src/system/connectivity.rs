//! Native OS connectivity checks — read the OS's own network state instead of
//! sending a probe request, so a routine "are we online" check costs zero bytes.
//!
//! Windows reads cached state from the Network List Manager, which Windows
//! itself populates by periodically probing Microsoft's NCSI endpoints (the
//! same signal behind the taskbar Wi-Fi icon) — querying it is a local COM
//! call, no network traffic from this process at all.
//!
//! macOS has no equivalent public API for a verified "is the internet actually
//! up" flag, so this only checks local route reachability via
//! `SCNetworkReachability` (zero network traffic, but a positive result just
//! means "there's a default route", not "the WAN is actually up" — e.g. a
//! captive portal with no real internet still reads as reachable). Callers
//! should treat `None` as "unknown, fall back to an active probe".

#[cfg(windows)]
pub fn check_native() -> Option<bool> {
    use windows::Win32::Networking::NetworkListManager::{INetworkListManager, NetworkListManager};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    // COM apartment is per-thread; `spawn_blocking` (see commands/system.rs)
    // hands this a fresh thread-pool thread with no apartment by default.
    struct ComGuard(bool);
    impl ComGuard {
        fn new() -> Self {
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            ComGuard(initialized)
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let _com = ComGuard::new();
    unsafe {
        let manager: INetworkListManager =
            CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok()?;
        let connected = manager.IsConnectedToInternet().ok()?;
        Some(connected.0 != 0)
    }
}

#[cfg(target_os = "macos")]
pub fn check_native() -> Option<bool> {
    use core::ffi::c_void;

    type ScNetworkReachabilityRef = *const c_void;
    type CfTypeRef = *const c_void;
    type ScNetworkReachabilityFlags = u32;

    const REACHABLE: u32 = 1 << 1;
    const CONNECTION_REQUIRED: u32 = 1 << 2;

    #[link(name = "SystemConfiguration", kind = "framework")]
    extern "C" {
        fn SCNetworkReachabilityCreateWithAddress(
            allocator: CfTypeRef,
            address: *const libc::sockaddr,
        ) -> ScNetworkReachabilityRef;
        fn SCNetworkReachabilityGetFlags(
            target: ScNetworkReachabilityRef,
            flags: *mut ScNetworkReachabilityFlags,
        ) -> u8;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CfTypeRef);
    }

    unsafe {
        // The zero address (0.0.0.0) is Apple's documented pattern for a
        // generic "is there any network route at all" check — it deliberately
        // avoids resolving a hostname, which would mean a DNS lookup.
        let mut addr: libc::sockaddr_in = std::mem::zeroed();
        addr.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
        addr.sin_family = libc::AF_INET as u8;

        let target = SCNetworkReachabilityCreateWithAddress(
            std::ptr::null(),
            &addr as *const libc::sockaddr_in as *const libc::sockaddr,
        );
        if target.is_null() {
            return None;
        }

        let mut flags: ScNetworkReachabilityFlags = 0;
        let ok = SCNetworkReachabilityGetFlags(target, &mut flags);
        CFRelease(target as CfTypeRef);

        if ok == 0 {
            return None;
        }

        let reachable = flags & REACHABLE != 0;
        let needs_connection = flags & CONNECTION_REQUIRED != 0;
        Some(reachable && !needs_connection)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn check_native() -> Option<bool> {
    None
}
