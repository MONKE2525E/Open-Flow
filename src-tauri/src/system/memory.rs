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
                let bytes = unsafe {
                    libc::proc_listchildpids(
                        ppid as libc::pid_t,
                        buf.as_mut_ptr() as *mut core::ffi::c_void,
                        (buf.len() * std::mem::size_of::<libc::pid_t>()) as i32,
                    )
                };

                if bytes <= 0 {
                    break;
                }

                let bytes = bytes as usize;
                let count = bytes / std::mem::size_of::<libc::pid_t>();
                for pid in buf.into_iter().take(count) {
                    let pid = pid as i32;
                    if pid > 0 && pid != ppid {
                        found.insert(pid);
                    }
                }

                if bytes < capacity * std::mem::size_of::<libc::pid_t>() {
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
