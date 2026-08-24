use std::process::Stdio;
use std::time::{Duration, Instant};

/// System-wide RAM availability (not just this process) — used to decide
/// whether to proactively unload local models under memory pressure rather
/// than waiting for their configured idle timeout.
#[derive(Clone, Copy, Debug)]
pub struct SystemMemoryStatus {
    pub available_mb: u64,
    pub total_mb: u64,
    pub load_percent: u32,
}

pub fn system_memory_status() -> Option<SystemMemoryStatus> {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status).is_ok() {
            Some(SystemMemoryStatus {
                available_mb: status.ullAvailPhys / (1024 * 1024),
                total_mb: status.ullTotalPhys / (1024 * 1024),
                load_percent: status.dwMemoryLoad,
            })
        } else {
            None
        }
    }

    #[cfg(target_os = "macos")]
    {
        let total_mb = macos_total_memory_bytes()? / (1024 * 1024);
        let available_mb = macos_free_memory_bytes()? / (1024 * 1024);
        let load_percent = if total_mb > 0 {
            (((total_mb.saturating_sub(available_mb)) as f64 / total_mb as f64) * 100.0) as u32
        } else {
            0
        };
        Some(SystemMemoryStatus {
            available_mb,
            total_mb,
            load_percent,
        })
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    None
}

#[cfg(target_os = "macos")]
fn macos_total_memory_bytes() -> Option<u64> {
    let output = std::process::Command::new("sysctl")
        .arg("-n")
        .arg("hw.memsize")
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

#[cfg(target_os = "macos")]
fn macos_free_memory_bytes() -> Option<u64> {
    let output = std::process::Command::new("vm_stat").output().ok()?;
    parse_vm_stat_free_bytes(&String::from_utf8_lossy(&output.stdout))
}

/// Parses `vm_stat`'s output. Page size varies by architecture (4096 bytes
/// on Intel, 16384 on Apple Silicon) and is reported in the header line, so
/// it must be parsed rather than assumed.
///
/// Sums "Pages free" with "Pages inactive/purgeable/speculative" (the same
/// pages Activity Monitor counts toward "Memory Used" being *not* full, i.e.
/// reclaimable without pressure) rather than "Pages free" alone. Darwin
/// aggressively fills otherwise-idle RAM with disk cache and keeps "Pages
/// free" intentionally low — often under 200 MB even with ample headroom —
/// so free-only would make `detect_resource_pressure()` read "critical" on
/// a healthy Mac almost permanently.
#[cfg(target_os = "macos")]
fn parse_vm_stat_free_bytes(text: &str) -> Option<u64> {
    let page_size: u64 = text
        .lines()
        .find(|line| line.contains("page size of "))?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let page_count = |label: &str| -> u64 {
        text.lines()
            .find(|line| line.trim_start().starts_with(label))
            .and_then(|line| line.split(':').nth(1))
            .map(|value| value.trim().trim_end_matches('.'))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    };
    let free_pages = page_count("Pages free:");
    if free_pages == 0
        && !text
            .lines()
            .any(|line| line.trim_start().starts_with("Pages free:"))
    {
        return None;
    }
    let reclaimable_pages = free_pages
        + page_count("Pages inactive:")
        + page_count("Pages purgeable:")
        + page_count("Pages speculative:");
    Some(reclaimable_pages * page_size)
}

/// NVIDIA-only VRAM check via `nvidia-smi` (same tool used for GPU backend
/// detection in local_llm::binary). No reliable cross-vendor equivalent
/// exists without vendor SDKs (AMD/Intel), so this returns an empty `Vec`
/// on non-NVIDIA systems or detection failure — callers must treat that as
/// "no signal", not "VRAM is fine". One entry per GPU, in `nvidia-smi`'s
/// own enumeration order — multi-GPU systems are common enough (and
/// `CUDA_VISIBLE_DEVICES`/device selection varies enough) that we check
/// every detected GPU rather than assuming index 0 is the relevant one.
#[derive(Clone, Copy, Debug)]
pub struct GpuVramStatus {
    pub index: u32,
    pub used_mb: u64,
    pub total_mb: u64,
}

const NVIDIA_SMI_TIMEOUT: Duration = Duration::from_secs(2);

pub fn gpu_vram_statuses() -> Vec<GpuVramStatus> {
    let mut command = std::process::Command::new("nvidia-smi");
    command
        .arg("--query-gpu=index,memory.used,memory.total")
        .arg("--format=csv,noheader,nounits");
    let Some(output) = run_with_timeout(command, NVIDIA_SMI_TIMEOUT) else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_nvidia_smi_memory_csv(&String::from_utf8_lossy(&output.stdout))
}

/// Runs `command`, killing it if it hasn't exited within `timeout`. Diagnostic
/// CLI tools (nvidia-smi included) are expected to respond in well under a
/// second; this exists purely so a wedged driver or hung process can never
/// stall the periodic resource-pressure check indefinitely.
fn run_with_timeout(
    mut command: std::process::Command,
    timeout: Duration,
) -> Option<std::process::Output> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        // Hardware probes are background work. CREATE_NO_WINDOW prevents
        // nvidia-smi from flashing a console over the app on Windows.
        command.creation_flags(0x08000000);
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .ok()?;

    // Drained on a dedicated thread concurrently with the wait loop below —
    // if the child wrote more than the OS pipe buffer (~64KB) before
    // exiting and nobody was reading it in the meantime, it would block
    // inside its own write() call forever, so try_wait() below would never
    // see it exit and this would stall for the full timeout on every call
    // instead of returning as soon as the child actually finishes.
    let stdout_handle = child.stdout.take().map(|mut out| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            buf
        })
    });

    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_handle
                    .and_then(|handle| handle.join().ok())
                    .unwrap_or_default();
                return Some(std::process::Output {
                    status,
                    stdout,
                    stderr: Vec::new(),
                });
            }
            Ok(None) => {
                if started_at.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn parse_nvidia_smi_memory_csv(text: &str) -> Vec<GpuVramStatus> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split(',').map(str::trim);
            let index: u32 = parts.next()?.parse().ok()?;
            let used_mb: u64 = parts.next()?.parse().ok()?;
            let total_mb: u64 = parts.next()?.parse().ok()?;
            Some(GpuVramStatus {
                index,
                used_mb,
                total_mb,
            })
        })
        .collect()
}

const LOW_RAM_THRESHOLD_MB: u64 = 1536;
const HIGH_RAM_LOAD_PERCENT: u32 = 92;
const LOW_VRAM_THRESHOLD_MB: u64 = 512;
const HIGH_VRAM_LOAD_PERCENT: u64 = 92;

/// Returns `Some(reason)` describing why the system looks low on RAM or
/// (NVIDIA) VRAM right now, for deciding whether to proactively unload
/// local models rather than waiting out their idle timeout — e.g.
/// launching a demanding game shouldn't have to wait out a 15-minute idle
/// timer before Verenu gives back the RAM/VRAM its local models hold. This
/// does blocking I/O (process spawn with a bounded wait); callers on an
/// async runtime should run it via `spawn_blocking`.
pub fn detect_resource_pressure() -> Option<String> {
    if let Some(mem) = system_memory_status() {
        if mem.available_mb < LOW_RAM_THRESHOLD_MB || mem.load_percent >= HIGH_RAM_LOAD_PERCENT {
            return Some(format!(
                "system RAM low: {} MB available of {} MB ({}% used)",
                mem.available_mb, mem.total_mb, mem.load_percent
            ));
        }
    }
    // Every detected GPU is checked independently (not aggregated) so a
    // single critically-low GPU is never masked by averaging it against an
    // idle one elsewhere in the system.
    for vram in gpu_vram_statuses() {
        if vram.total_mb == 0 {
            continue;
        }
        let free_mb = vram.total_mb.saturating_sub(vram.used_mb);
        let load_percent = (vram.used_mb * 100) / vram.total_mb;
        if free_mb < LOW_VRAM_THRESHOLD_MB || load_percent >= HIGH_VRAM_LOAD_PERCENT {
            return Some(format!(
                "GPU {} VRAM low: {free_mb} MB free of {} MB ({load_percent}% used)",
                vram.index, vram.total_mb
            ));
        }
    }
    None
}

#[cfg(test)]
mod pressure_tests {
    use super::*;

    #[test]
    fn nvidia_smi_csv_parses_used_and_total() {
        let parsed = parse_nvidia_smi_memory_csv("0, 512, 8192\n");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].index, 0);
        assert_eq!(parsed[0].used_mb, 512);
        assert_eq!(parsed[0].total_mb, 8192);
    }

    #[test]
    fn nvidia_smi_csv_parses_multiple_gpus() {
        let parsed = parse_nvidia_smi_memory_csv("0, 512, 8192\n1, 7900, 8192\n");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].index, 1);
        assert_eq!(parsed[1].used_mb, 7900);
    }

    #[test]
    fn nvidia_smi_csv_rejects_garbage() {
        assert!(parse_nvidia_smi_memory_csv("not a csv line").is_empty());
        assert!(parse_nvidia_smi_memory_csv("").is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn vm_stat_parses_intel_page_size() {
        let sample = "Mach Virtual Memory Statistics: (page size of 4096 bytes)\n\
            Pages free:                              100000.\n\
            Pages active:                             50000.\n";
        assert_eq!(parse_vm_stat_free_bytes(sample), Some(100_000 * 4096));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn vm_stat_parses_apple_silicon_page_size() {
        let sample = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
            Pages free:                               20000.\n";
        assert_eq!(parse_vm_stat_free_bytes(sample), Some(20_000 * 16384));
    }

    #[test]
    fn detect_resource_pressure_thresholds_are_sane() {
        assert!(LOW_RAM_THRESHOLD_MB > 0);
        assert!(HIGH_RAM_LOAD_PERCENT <= 100);
        assert!(LOW_VRAM_THRESHOLD_MB > 0);
        assert!(HIGH_VRAM_LOAD_PERCENT <= 100);
    }
}

/// Returns the total resident private memory used by this process and all
/// WebView2 child processes (in MB), matching Task Manager's "Memory" column.
pub fn measure() -> u64 {
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return 0;

    // macOS: sum resident memory of this process and its WebView child processes
    // by walking the process tree (children are grandchildren of the main app,
    // same as WebView2 on Windows). Uses RSS, which includes some shared pages, so
    // it slightly over-reports versus Windows' private working set — informational.
    #[cfg(target_os = "macos")]
    {
        use libproc::libproc::proc_pid::pidinfo;
        use libproc::libproc::task_info::TaskAllInfo;
        use std::collections::{HashSet, VecDeque};

        let our_pid = std::process::id() as i32;

        let direct_children = |ppid: i32| -> Vec<i32> {
            let mut found = HashSet::new();
            let mut capacity = 32usize;

            loop {
                let mut buf = vec![0 as libc::pid_t; capacity];
                let num_pids = unsafe {
                    extern "C" {
                        fn proc_listchildpids(
                            ppid: libc::pid_t,
                            buffer: *mut libc::c_void,
                            buffersize: libc::c_int,
                        ) -> libc::c_int;
                    }
                    proc_listchildpids(
                        ppid as libc::pid_t,
                        buf.as_mut_ptr() as *mut core::ffi::c_void,
                        (buf.len() * std::mem::size_of::<libc::pid_t>()) as i32,
                    )
                };

                if num_pids <= 0 {
                    break;
                }

                let num_pids = num_pids as usize;
                let count = num_pids.min(buf.len());
                for pid in buf.into_iter().take(count) {
                    if pid > 0 && pid != ppid {
                        found.insert(pid);
                    }
                }

                if count < capacity {
                    break;
                }

                capacity *= 2;
                if capacity > 4096 {
                    break;
                }
            }

            found.into_iter().collect()
        };

        let resident = |pid: i32| -> u64 {
            pidinfo::<TaskAllInfo>(pid, 0)
                .map(|t| t.ptinfo.pti_resident_size)
                .unwrap_or(0)
        };

        let mut total = resident(our_pid);
        let mut seen: HashSet<i32> = HashSet::new();
        seen.insert(our_pid);
        let mut queue: VecDeque<i32> = VecDeque::new();
        queue.extend(direct_children(our_pid));
        while let Some(pid) = queue.pop_front() {
            if seen.insert(pid) {
                total += resident(pid);
                queue.extend(direct_children(pid));
            }
        }
        total / (1024 * 1024)
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use std::collections::{HashMap, HashSet, VecDeque};
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
        };
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetCurrentProcessId, OpenProcess, PROCESS_QUERY_INFORMATION,
            PROCESS_VM_READ,
        };

        // PrivateWorkingSetSize from EX2 matches Task Manager's "Memory" column exactly
        // (resident private pages only). PrivateUsage/EX would include pre-committed
        // virtual memory that Chromium/WebView2 reserves but hasn't touched yet.
        let private_bytes = |h| -> usize {
            let mut pmc: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
            if GetProcessMemoryInfo(
                h,
                &mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut PROCESS_MEMORY_COUNTERS,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
            )
            .is_ok()
            {
                pmc.PrivateWorkingSetSize
            } else {
                0
            }
        };

        let our_pid = GetCurrentProcessId();
        let mut total = private_bytes(GetCurrentProcess());
        let mut counted_pids = HashSet::new();
        counted_pids.insert(our_pid);

        // Build a full parent→children map in one snapshot pass, then BFS the
        // entire subtree. WebView2 renderer/GPU processes are grandchildren (children
        // of the browser process), so a single-level walk misses most of the memory.
        let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    let pid = entry.th32ProcessID;
                    let ppid = entry.th32ParentProcessID;
                    if pid != ppid {
                        children_map.entry(ppid).or_default().push(pid);
                    }
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            CloseHandle(snap).ok();
        }

        let mut queue = VecDeque::new();
        if let Some(kids) = children_map.get(&our_pid) {
            queue.extend(kids.iter().copied());
        }
        while let Some(pid) = queue.pop_front() {
            if counted_pids.insert(pid) {
                if let Ok(h) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
                {
                    total += private_bytes(h);
                    CloseHandle(h).ok();
                }
            }
            if let Some(kids) = children_map.get(&pid) {
                queue.extend(kids.iter().copied());
            }
        }

        (total / (1024 * 1024)) as u64
    }
}

pub fn free_bytes_for_path(path: &std::path::Path) -> Result<u64, String> {
    let path = path
        .ancestors()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| "No existing ancestor for disk space query".to_string())?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let mut free_bytes_available: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let result = unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(wide.as_ptr()),
                Some(&mut free_bytes_available),
                Some(&mut total_bytes),
                Some(&mut total_free_bytes),
            )
        };
        result
            .map(|_| free_bytes_available)
            .map_err(|_| "Failed to read free disk space".to_string())
    }

    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let c_path =
            CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid path".to_string())?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
        if rc != 0 {
            return Err("Failed to read free disk space".to_string());
        }
        Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = path;
        Ok(u64::MAX)
    }
}
