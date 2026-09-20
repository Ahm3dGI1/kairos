//! Kairos — the Windows 11 shell.
//!
//! A Tauri app over `kairos-core`. This crate owns windows, the tray, the global
//! hotkey and the IPC surface; every decision about what a task *means* belongs
//! to the core, so that a Linux or mobile client behaves identically.

// Release builds are a desktop app, not a console program.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod daily_commands;
mod reminders;
mod settings_commands;
mod shell;
mod state;
mod watcher;
mod workout_commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![shell::AUTOSTART_FLAG]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            state::init(&handle)?;
            shell::setup(&handle)?;
            reminders::start(&handle);
            watcher::start(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            use tauri::Manager;
            // Closing a window parks the app in the tray instead of quitting —
            // a todo app that vanishes when you close its window is a todo app
            // that stops reminding you. Quit is on the tray menu.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Unless the user asked for the X to mean what it says.
                if !settings_commands::current(window.app_handle()).close_to_tray {
                    return;
                }
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
            commands::redo,
            commands::can_undo,
            commands::can_redo,
            commands::projects,
            commands::tags,
            commands::calendar_month,
            commands::today_date,
            commands::summary,
            commands::current_month,
            commands::view_counts,
            daily_commands::agenda,
            daily_commands::habit_month,
            daily_commands::toggle_habit,
            daily_commands::add_habit,
            daily_commands::rename_habit,
            daily_commands::delete_habit,
            daily_commands::set_day_metric,
            daily_commands::month_journal,
            daily_commands::save_journal_entry,
            daily_commands::delete_journal_entry,
            daily_commands::current_month_pair,
            workout_commands::workout_page,
            workout_commands::add_routine,
            workout_commands::rename_routine,
            workout_commands::delete_routine,
            workout_commands::add_exercise,
            workout_commands::rename_exercise,
            workout_commands::delete_exercise,
            workout_commands::start_session,
            workout_commands::save_set,
            workout_commands::save_session_note,
            workout_commands::delete_session,
            settings_commands::settings,
            settings_commands::settings_values,
            settings_commands::set_setting,
            settings_commands::vault_info,
            settings_commands::reload_vault,
            settings_commands::rewrite_vault,
            settings_commands::open_vault,
            shell::hide_window,
            shell::toggle_widget_command,
            shell::show_main_window,
            shell::toggle_sticky_pin,
            shell::sticky_pinned,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Kairos");
}
