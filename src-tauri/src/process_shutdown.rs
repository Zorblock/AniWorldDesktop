#[cfg(windows)]
mod platform {
    use std::{
        ffi::OsString,
        mem::size_of,
        os::windows::ffi::OsStringExt,
        path::{Path, PathBuf},
        time::Duration,
    };
    use windows::{
        core::{BOOL, PWSTR},
        Win32::{
            Foundation::{
                CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM,
                WAIT_OBJECT_0, WPARAM,
            },
            System::{
                Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                    TH32CS_SNAPPROCESS,
                },
                Threading::{
                    CreateMutexW, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
                    WaitForSingleObject, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
                },
            },
            UI::WindowsAndMessaging::{
                EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
            },
        },
    };

    const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
    const FORCED_SHUTDOWN_TIMEOUT_MS: u32 = 2_000;
    const UPDATE_MUTEX_NAME: windows::core::PCWSTR =
        windows::core::w!("Local\\Zorblock.AniWorldDesktop.Updater");

    struct OwnedHandle(HANDLE);

    // Windows kernel handles may be owned and closed from a different thread.
    unsafe impl Send for OwnedHandle {}

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    pub struct UpdateLock {
        _handle: OwnedHandle,
    }

    struct OtherProcess {
        pid: u32,
        handle: OwnedHandle,
    }

    pub fn acquire_update_lock() -> Result<Option<UpdateLock>, String> {
        let handle = unsafe { CreateMutexW(None, false, UPDATE_MUTEX_NAME) }
            .map_err(|error| format!("Could not create the update lock: {error}"))?;
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let handle = OwnedHandle(handle);

        if already_exists {
            Ok(None)
        } else {
            Ok(Some(UpdateLock { _handle: handle }))
        }
    }

    pub fn close_other_instances() -> Result<usize, String> {
        let processes = find_other_instances()?;
        if processes.is_empty() {
            return Ok(0);
        }

        for process in &processes {
            let _ = unsafe {
                EnumWindows(Some(close_window_for_process), LPARAM(process.pid as isize))
            };
        }

        std::thread::sleep(GRACEFUL_SHUTDOWN_TIMEOUT);

        for process in &processes {
            if unsafe { WaitForSingleObject(process.handle.0, 0) } == WAIT_OBJECT_0 {
                continue;
            }

            let force_handle =
                unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, process.pid) }
                    .map(OwnedHandle)
                    .map_err(|error| {
                        format!(
                            "Could not close AniWorld Desktop process {}: {error}",
                            process.pid
                        )
                    })?;

            unsafe { TerminateProcess(force_handle.0, 0) }.map_err(|error| {
                format!(
                    "Could not close AniWorld Desktop process {}: {error}",
                    process.pid
                )
            })?;

            if unsafe { WaitForSingleObject(force_handle.0, FORCED_SHUTDOWN_TIMEOUT_MS) }
                != WAIT_OBJECT_0
            {
                return Err(format!(
                    "AniWorld Desktop process {} did not close in time",
                    process.pid
                ));
            }
        }

        Ok(processes.len())
    }

    fn find_other_instances() -> Result<Vec<OtherProcess>, String> {
        let current_pid = std::process::id();
        let current_path = normalized_path(
            &std::env::current_exe()
                .map_err(|error| format!("Could not determine the application path: {error}"))?,
        );
        let current_name = Path::new(&current_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("aniworld-desktop.exe")
            .to_owned();

        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map(OwnedHandle)
            .map_err(|error| format!("Could not enumerate running applications: {error}"))?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut processes = Vec::new();

        if unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_err() {
            return Ok(processes);
        }

        loop {
            if entry.th32ProcessID != current_pid
                && process_name(&entry).eq_ignore_ascii_case(&current_name)
            {
                let handle = unsafe {
                    OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                        false,
                        entry.th32ProcessID,
                    )
                }
                .map(OwnedHandle)
                .map_err(|error| {
                    format!(
                        "Could not inspect AniWorld Desktop process {}: {error}",
                        entry.th32ProcessID
                    )
                })?;

                if normalized_path(&process_path(handle.0)?) == current_path {
                    processes.push(OtherProcess {
                        pid: entry.th32ProcessID,
                        handle,
                    });
                }
            }

            if unsafe { Process32NextW(snapshot.0, &mut entry) }.is_err() {
                break;
            }
        }

        Ok(processes)
    }

    fn process_name(entry: &PROCESSENTRY32W) -> String {
        let length = entry
            .szExeFile
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(entry.szExeFile.len());
        OsString::from_wide(&entry.szExeFile[..length])
            .to_string_lossy()
            .into_owned()
    }

    fn process_path(handle: HANDLE) -> Result<PathBuf, String> {
        let mut buffer = vec![0_u16; 32_768];
        let mut length = buffer.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
        }
        .map_err(|error| format!("Could not determine a running application path: {error}"))?;
        Ok(PathBuf::from(OsString::from_wide(
            &buffer[..length as usize],
        )))
    }

    fn normalized_path(path: &Path) -> String {
        path.canonicalize()
            .unwrap_or_else(|_| path.to_owned())
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .to_ascii_lowercase()
    }

    unsafe extern "system" fn close_window_for_process(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let mut pid = 0_u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == parameter.0 as u32 {
            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        BOOL(1)
    }
}

#[cfg(not(windows))]
mod platform {
    pub struct UpdateLock;

    pub fn acquire_update_lock() -> Result<Option<UpdateLock>, String> {
        Ok(Some(UpdateLock))
    }

    pub fn close_other_instances() -> Result<usize, String> {
        Ok(0)
    }
}

pub use platform::{acquire_update_lock, close_other_instances};
