//! Master Todo — the Windows 11 shell.
//!
//! A Tauri app over `mtodo-core`. This crate owns windows, the tray, the global
//! hotkey and the IPC surface; every decision about what a task *means* belongs
//! to the core, so that a Linux or mobile client behaves identically.

// Release builds are a desktop app, not a console program.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod shell;
mod state;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            state::init(&handle)?;
            shell::setup(&handle)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing a window parks the app in the tray instead of quitting —
            // a todo app that vanishes when you close its window is a todo app
            // that stops reminding you. Quit is on the tray menu.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::get_task,
            commands::preview_line,
            commands::quick_add,
            commands::complete_task,
            commands::uncomplete_task,
            commands::delete_task,
            commands::update_task,
            commands::add_subtask,
            commands::toggle_subtask,
            commands::skip_occurrence,
            commands::reschedule_occurrence,
            commands::undo,
            commands::can_undo,
            commands::projects,
            commands::tags,
            commands::calendar_month,
            commands::today_date,
            commands::summary,
            commands::current_month,
            commands::view_counts,
            shell::hide_window,
            shell::toggle_widget_command,
            shell::show_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Master Todo");
}
