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
        use libproc::libproc::bsd_info::BSDInfo;
        use libproc::libproc::proc_pid::pidinfo;
        use libproc::libproc::task_info::TaskAllInfo;
        use libproc::processes::{pids_by_type, ProcFilter};
        use std::collections::{HashMap, HashSet, VecDeque};

        let our_pid = std::process::id() as i32;

        let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
        if let Ok(all) = pids_by_type(ProcFilter::All) {
            for pid in all {
                let pid = pid as i32;
                if pid <= 0 {
                    continue;
                }
                if let Ok(info) = pidinfo::<BSDInfo>(pid, 0) {
                    children.entry(info.pbi_ppid as i32).or_default().push(pid);
                }
            }
        }

        let resident = |pid: i32| -> u64 {
            pidinfo::<TaskAllInfo>(pid, 0)
                .map(|t| t.ptinfo.pti_resident_size)
                .unwrap_or(0)
        };

        let mut total = resident(our_pid);
        let mut seen: HashSet<i32> = HashSet::new();
        seen.insert(our_pid);
        let mut queue: VecDeque<i32> = VecDeque::new();
        if let Some(kids) = children.get(&our_pid) {
            queue.extend(kids.iter().copied());
        }
        while let Some(pid) = queue.pop_front() {
            if seen.insert(pid) {
                total += resident(pid);
                if let Some(kids) = children.get(&pid) {
                    queue.extend(kids.iter().copied());
                }
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
