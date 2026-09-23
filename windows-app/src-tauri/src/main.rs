// Release builds are a desktop app, not a console program.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod reminders;
mod shell;
mod state;
mod watcher;
mod widgets;

fn main() {
    use tauri::Manager;
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![shell::AUTOSTART_FLAG]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(widgets::Windows::default());
            state::init(&handle)?;
            shell::setup(&handle)?;
            if let Err(error) = widgets::startup(&handle) {
                eprintln!("Could not restore widgets: {error}");
            }
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
                let app = window.app_handle();
                if window.label().starts_with("widget-") {
                    widgets::closed(app, window.label());
                    return;
                }
                let settings = commands::settings::current(app);
                if window.label() == "main" && !(settings.close_to_tray && settings.tray_icon) {
                    app.exit(0);
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::tasks::list_tasks,
            commands::tasks::preview_line,
            commands::tasks::quick_add,
            commands::tasks::complete_task,
            commands::tasks::uncomplete_task,
            commands::tasks::delete_task,
            commands::tasks::update_task,
            commands::tasks::add_subtask,
            commands::tasks::toggle_subtask,
            commands::tasks::skip_occurrence,
            commands::tasks::reschedule_occurrence,
            commands::tasks::undo,
            commands::tasks::redo,
            commands::tasks::projects,
            commands::tasks::tags,
            commands::tasks::calendar_month,
            commands::tasks::today_date,
            commands::tasks::summary,
            commands::tasks::current_month,
            commands::daily::agenda,
            commands::daily::habit_month,
            commands::daily::toggle_habit,
            commands::daily::add_habit,
            commands::daily::habits,
            commands::daily::edit_habit,
            commands::daily::delete_habit,
            commands::daily::set_habit_value,
            commands::daily::month_journal,
            commands::daily::save_journal_entry,
            commands::daily::current_month_pair,
            commands::workout::workout_page,
            commands::workout::add_routine,
            commands::workout::rename_routine,
            commands::workout::delete_routine,
            commands::workout::add_exercise,
            commands::workout::rename_exercise,
            commands::workout::delete_exercise,
            commands::workout::start_session,
            commands::workout::save_set,
            commands::workout::save_session_note,
            commands::workout::delete_session,
            commands::settings::settings,
            commands::settings::settings_values,
            commands::settings::set_setting,
            commands::settings::vault_info,
            commands::settings::reload_vault,
            commands::settings::rewrite_vault,
            commands::settings::open_vault,
            widgets::create_widget,
            widgets::widget_context,
            widgets::update_widget_context,
            widgets::pin_widget,
            widgets::widget_layout,
            widgets::save_widget_layout,
            widgets::set_widget_restore,
            widgets::restore_widget_layout,
            shell::hide_window,
            shell::toggle_widget_command,
            shell::show_main_window,
            shell::toggle_sticky_pin,
            shell::sticky_pinned,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Kairos");
}
