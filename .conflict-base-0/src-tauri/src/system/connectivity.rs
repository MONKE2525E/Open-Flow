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
//! captive portal with no real internet still reads as reachable).
//!
//! Both platforms can also report false negatives (VPNs, enterprise proxies,
//! or NCSI getting stuck) — so callers should only trust a confirmed
//! `Some(true)` and treat `Some(false)`/`None` alike as "unknown, fall back
//! to an active probe" rather than concluding "offline".

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
        Some(connected.as_bool())
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
        CFRelease(target);

        if ok == 0 {
            return None;
        }

        let reachable = (flags & REACHABLE) != 0;
        let needs_connection = (flags & CONNECTION_REQUIRED) != 0;
        Some(reachable && !needs_connection)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn check_native() -> Option<bool> {
    None
}

/// Active connectivity probe, used only after a real request has already
/// failed, to distinguish "the user's internet is down" from "one provider is
/// down". Unlike [`check_native`] this actually sends a request, so it costs a
/// little traffic and is deliberately gated to the failure path.
///
/// Ordering follows the privacy preference: probe Verenu's own endpoint first
/// when service checks are enabled, then google.com as an independent second
/// opinion — covering both "Verenu is down but the user is fine" and the case
/// where the user disabled Verenu checks entirely. Returns true only when
/// every attempted probe failed (i.e. the connection itself is the problem).
pub async fn confirm_offline(verenu_checks_enabled: bool) -> bool {
    const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

    if verenu_checks_enabled && probe("https://api.verenu.com/v1/health", PROBE_TIMEOUT).await {
        return false;
    }
    !probe("https://www.google.com", PROBE_TIMEOUT).await
}

/// A probe counts as "online" when any HTTP response arrives at all — even a
/// 4xx/5xx proves the host was reachable, which is all a connectivity check
/// needs. Only a failed send (DNS/connect error, timeout) reads as "offline".
async fn probe(url: &str, timeout: std::time::Duration) -> bool {
    crate::api::client::get()
        .get(url)
        .header("User-Agent", "verenu")
        .timeout(timeout)
        .send()
        .await
        .is_ok()
}
