use crate::models::ClaudeActivity;
use sysinfo::System;

pub fn claude_activity_blocks_cleanup(activity: ClaudeActivity) -> bool {
    activity != ClaudeActivity::NotDetected
}

pub fn claude_cleanup_block_message(activity: ClaudeActivity) -> String {
    match activity {
        ClaudeActivity::Background => "Claude background processes are still running and may be locking cache files. Fully quit Claude from the tray or Task Manager, then try again.".to_string(),
        ClaudeActivity::Window => "Claude Desktop is running. Fully close Claude, or explicitly enable cleanup while Claude is running.".to_string(),
        ClaudeActivity::NotDetected => "Claude is not detected.".to_string(),
    }
}

pub fn claude_activity() -> ClaudeActivity {
    let mut system = System::new_all();
    system.refresh_processes();
    let current_pid = std::process::id().to_string();
    match std::env::consts::OS {
        "windows" => {
            let process_ids = system
                .processes()
                .iter()
                .filter_map(|(pid, process)| {
                    if is_claude_desktop_process_name_for_os(
                        process.name(),
                        &pid.to_string(),
                        &current_pid,
                        "windows",
                    ) {
                        pid.to_string().parse::<u32>().ok()
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            if process_ids.is_empty() {
                ClaudeActivity::NotDetected
            } else if windows_has_visible_window_for_processes(&process_ids) {
                ClaudeActivity::Window
            } else {
                ClaudeActivity::Background
            }
        }
        _ => {
            if system.processes().iter().any(|(pid, process)| {
                is_claude_desktop_process_name_for_os(
                    process.name(),
                    &pid.to_string(),
                    &current_pid,
                    std::env::consts::OS,
                )
            }) {
                ClaudeActivity::Window
            } else {
                ClaudeActivity::NotDetected
            }
        }
    }
}

pub fn is_claude_desktop_process_name_for_os(
    process_name: &str,
    process_pid: &str,
    current_pid: &str,
    os: &str,
) -> bool {
    if process_pid == current_pid {
        return false;
    }
    let name = process_name.trim();
    match os {
        "windows" => name.eq_ignore_ascii_case("claude") || name.eq_ignore_ascii_case("claude.exe"),
        "macos" => name == "Claude",
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn windows_has_visible_window_for_processes(process_ids: &[u32]) -> bool {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };
    struct SearchState<'a> {
        process_ids: &'a [u32],
        found: bool,
    }
    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam as *mut SearchState<'_>);
        if state.found || IsWindowVisible(hwnd) == 0 || GetWindowTextLengthW(hwnd) <= 0 {
            return 1;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if state.process_ids.contains(&pid) {
            state.found = true;
            return 0;
        }
        1
    }
    if process_ids.is_empty() {
        return false;
    }
    let mut state = SearchState {
        process_ids,
        found: false,
    };
    unsafe {
        EnumWindows(Some(callback), &mut state as *mut SearchState<'_> as LPARAM);
    }
    state.found
}

#[cfg(not(target_os = "windows"))]
fn windows_has_visible_window_for_processes(_process_ids: &[u32]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_process_names_only() {
        assert!(is_claude_desktop_process_name_for_os(
            "Claude.exe",
            "100",
            "42",
            "windows"
        ));
        assert!(!is_claude_desktop_process_name_for_os(
            "Claude Cache Warden",
            "100",
            "42",
            "windows"
        ));
        assert!(!is_claude_desktop_process_name_for_os(
            "Claude", "42", "42", "macos"
        ));
    }
}
