use crate::state::{self, AppState};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last_error = String::new();
        let mut last_settings = Vec::new();
        loop {
            std::thread::sleep(Duration::from_millis(1500));
            let Some(state) = app.try_state::<AppState>() else { continue };
            match state::import(&state) {
                Ok(changed) => {
                    last_error.clear();
                    if changed {
                        state::notify_changed(&app);
                    }
                }
                Err(error) => {
                    if error != last_error {
                        let _ = app.emit("storage-error", &error);
                        last_error = error;
                    }
                }
            }
            if let Ok(bytes) = std::fs::read(state.paths.settings_file()) {
                if bytes != last_settings {
                    if let Ok(settings) = serde_json::from_slice::<kairos_core::Settings>(&bytes) {
                        if let Ok(mut current) = state.settings.lock() {
                            if *current != settings {
                                if current.start_on_login != settings.start_on_login {
                                    if let Err(error) =
                                        crate::shell::sync_autostart(&app, settings.start_on_login)
                                    {
                                        let _ = app.emit("storage-error", error);
                                    }
                                }
                                *current = settings;
                                let _ = app.emit(crate::commands::settings::SETTINGS_CHANGED, ());
                            }
                        }
                        last_settings = bytes;
                    }
                }
            }
        }
    });
}
