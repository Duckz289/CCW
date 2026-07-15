mod cleanup;
mod commands;
mod models;
mod platform;
mod process;
mod quarantine;
mod safety;
mod scanner;
mod scheduler;
mod state;

use state::AppState;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = Arc::new(AppState::load(app.handle().clone()));
            let minimized = platform::launched_minimized();
            app.manage(state);
            build_tray(app)?;
            scheduler::start_scheduler(app.handle().clone());
            if minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_cache,
            commands::preview_cleanup,
            commands::clean_cache,
            commands::get_scheduler_settings,
            commands::save_scheduler_settings,
            commands::get_clean_history,
            commands::evaluate_growth_alert,
            commands::export_report,
            commands::get_claude_running,
            commands::get_claude_activity,
            commands::list_quarantine_entries,
            commands::restore_quarantine_entry,
            commands::permanently_delete_quarantine_entry,
            commands::clear_expired_quarantine,
            commands::open_in_file_manager,
            commands::open_export_location,
            commands::get_largest_items,
            commands::get_file_type_breakdown,
            commands::get_volume_status,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Claude Cache Warden");
}

fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "Scan", true, None::<&str>)?;
    let clean = MenuItem::with_id(app, "clean", "Run safe cleanup", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &scan, &clean, &quit])?;
    let mut builder = TrayIconBuilder::new();
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder
        .tooltip("Claude Cache Warden")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "scan" => {
                let _ = app.emit("scan-requested", ());
                show_main_window(app);
            }
            // The frontend opens the same preview dialog used by manual cleanup.
            "clean" => {
                let _ = app.emit("safe-cleanup-requested", ());
                show_main_window(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
