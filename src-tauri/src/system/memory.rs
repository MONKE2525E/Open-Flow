/// Returns the total resident private memory used by this process and all
/// WebView2 child processes (in MB), matching Task Manager's "Memory" column.
pub fn measure() -> u64 {
    #[cfg(not(target_os = "windows"))]
    return 0;

    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
        };
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetCurrentProcessId, OpenProcess,
            PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
        };
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
            PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };
        use std::collections::{HashMap, HashSet, VecDeque};

        // PrivateWorkingSetSize from EX2 matches Task Manager's "Memory" column exactly
        // (resident private pages only). PrivateUsage/EX would include pre-committed
        // virtual memory that Chromium/WebView2 reserves but hasn't touched yet.
        let private_bytes = |h| -> usize {
            let mut pmc: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
            if GetProcessMemoryInfo(
                h,
                &mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut PROCESS_MEMORY_COUNTERS,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
            ).is_ok() { pmc.PrivateWorkingSetSize } else { 0 }
        };

        let our_pid = GetCurrentProcessId();
        let mut total = private_bytes(GetCurrentProcess());
        let mut counted_pids = HashSet::new();
        counted_pids.insert(our_pid);

        // Build a full parent→children map in one snapshot pass, then BFS the
        // entire subtree. WebView2 renderer/GPU processes are grandchildren (children
        // of the browser process), so a single-level walk misses most of the memory.
        let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut webview_pids = Vec::new();
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
                    // Track WebView2 processes explicitly in case they're not in direct hierarchy
                    let name_wide = &entry.szExeFile;
                    let name_len = name_wide.iter().position(|&c| c == 0).unwrap_or(name_wide.len());
                    if name_len > 0 {
                        if let Ok(name) = String::from_utf16(&name_wide[..name_len]) {
                            let lower_name = name.to_lowercase();
                            if lower_name.contains("webview2") || lower_name.contains("msedgewebv") {
                                webview_pids.push(pid);
                            }
                        }
                    }
                    if Process32NextW(snap, &mut entry).is_err() { break; }
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
                if let Ok(h) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
                    total += private_bytes(h);
                    CloseHandle(h).ok();
                }
            }
            if let Some(kids) = children_map.get(&pid) {
                queue.extend(kids.iter().copied());
            }
        }

        // Also include any WebView2 processes that might not be in the direct tree
        // (e.g., from multi-window scenarios in release builds)
        for pid in webview_pids {
            if counted_pids.insert(pid) {
                if let Ok(h) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
                    total += private_bytes(h);
                    CloseHandle(h).ok();
                }
            }
        }

        (total / (1024 * 1024)) as u64
    }
}
