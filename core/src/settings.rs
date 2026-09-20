//! What the user has turned on, and where their files live.
//!
//! Settings are part of the vault, not the database: a preference you cannot
//! read without launching the app is a preference you cannot fix when the app
//! will not launch. `settings.json` sits beside the notes as plain JSON, and
//! anything missing from it falls back to the default — so a hand-edited file
//! that drops half its keys still loads.
//!
//! The list of switches lives here rather than in a client because every shell
//! should offer the same ones. A client renders [`Settings::describe`] and
//! calls [`Settings::set`]; it never hard-codes a key.

use serde::{Deserialize, Serialize};

/// Everything the user can turn on or off.
///
/// `#[serde(default)]` on the container means a missing key takes its default,
/// so adding a switch never invalidates an existing file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Pages
    /// Show the calendar in the rail.
    pub calendar_page: bool,
    /// Show the habit month in the rail.
    pub habits_page: bool,
    /// Show the workout book in the rail.
    pub workout_page: bool,
    /// Show the lists sidebar inside Tasks.
    pub lists_sidebar: bool,

    // Capture
    /// Tint recognized phrases in the capture field as you type.
    pub parse_pills: bool,
    /// Register the `Ctrl+Shift+Space` quick-add overlay.
    pub global_hotkey: bool,
    /// Keep the quick-add overlay open after adding, for a run of captures.
    pub hotkey_stays_open: bool,

    // Windows
    /// Offer the sticky note, and its tray entry.
    pub sticky_note: bool,
    /// Closing the last window parks the app in the tray instead of quitting.
    pub close_to_tray: bool,
    /// Show the tray icon at all.
    pub tray_icon: bool,
    /// Fire a Windows notification when a task's time arrives.
    pub reminders: bool,

    // Behaviour
    /// Ask before deleting a task. Off by default: undo is the safety net, and
    /// a dialog on every delete is the friction this project exists to remove.
    pub confirm_delete: bool,
    /// Show completed work in the task list.
    pub show_completed: bool,
    /// Weeks start on Monday. Off means Sunday.
    pub week_starts_monday: bool,

    // Appearance
    pub theme: Theme,

    // Storage
    /// Mirror everything into plain-text files, and read external edits back.
    pub vault_enabled: bool,
    /// Where those files live. Empty means the default beside the database.
    pub vault_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            calendar_page: true,
            habits_page: true,
            workout_page: true,
            lists_sidebar: true,
            parse_pills: true,
            global_hotkey: true,
            hotkey_stays_open: false,
            sticky_note: true,
            close_to_tray: true,
            tray_icon: true,
            reminders: true,
            confirm_delete: false,
            show_completed: false,
            week_starts_monday: true,
            theme: Theme::System,
            vault_enabled: true,
            vault_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

/// What kind of control a switch needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SettingKind {
    Toggle,
    /// One of a fixed set: the stored value, then the label to show.
    Choice {
        options: Vec<(String, String)>,
    },
    /// A filesystem path, with a placeholder describing the empty default.
    Path {
        placeholder: String,
    },
}

/// One switch, described well enough for a client to render it without knowing
/// what it means.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub key: &'static str,
    pub section: &'static str,
    pub label: &'static str,
    /// Why you would want it — shown under the label.
    pub detail: &'static str,
    #[serde(flatten)]
    pub kind: SettingKind,
    /// The current value, as JSON.
    pub value: serde_json::Value,
    /// Set when changing this needs the app restarted to take effect.
    pub restart: bool,
}

/// key, section, label, detail, needs-restart — in the order a settings page
/// should show them.
#[rustfmt::skip]
const SCHEMA: &[(&str, &str, &str, &str, bool)] = &[
    ("calendar_page", "Pages", "Calendar",
     "A month grid with recurring tasks expanded onto every day they fall on.", false),
    ("habits_page", "Pages", "Habits",
     "The habit month and the journal beside it.", false),
    ("workout_page", "Pages", "Workout",
     "Routines, exercises and the session grid.", false),
    ("lists_sidebar", "Pages", "Lists sidebar",
     "A second sidebar inside Tasks holding every project and tag in use.", false),

    ("parse_pills", "Capture", "Highlight what was parsed",
     "Tint recognized dates, times and repeats in the capture field. Backspace against a pill un-parses it either way.", false),
    ("global_hotkey", "Capture", "Global hotkey",
     "Ctrl+Shift+Space opens quick add from anywhere in Windows.", true),
    ("hotkey_stays_open", "Capture", "Quick add stays open",
     "Keep the overlay up after adding, for a run of captures. Shift+Enter does this on demand regardless.", false),

    ("sticky_note", "Windows", "Sticky note",
     "A small always-available panel of today's tasks.", false),
    ("tray_icon", "Windows", "Tray icon",
     "Hover for what is due today; click to open.", true),
    ("close_to_tray", "Windows", "Close to tray",
     "Closing the window parks the app instead of quitting it. Off means the X quits.", false),
    ("reminders", "Windows", "Reminders",
     "A notification when a task's time arrives.", false),

    ("show_completed", "Behaviour", "Show completed",
     "Keep finished work visible in the list instead of behind the toggle.", false),
    ("confirm_delete", "Behaviour", "Confirm before deleting",
     "Off by default — undo already covers a mistaken delete.", false),
    ("week_starts_monday", "Behaviour", "Weeks start on Monday",
     "Off starts them on Sunday.", false),

    ("theme", "Appearance", "Theme",
     "System follows Windows.", false),

    ("vault_enabled", "Storage", "Plain-text vault",
     "Mirror every task, habit and workout into readable files you can edit in any editor. The files are the real data; the database is a rebuildable index.", true),
    ("vault_path", "Storage", "Vault folder",
     "Where those files live.", true),
];

impl Settings {
    /// Reads a file, falling back to the defaults if it is missing or broken.
    ///
    /// A corrupt settings file must never stop the app starting — the worst it
    /// can do is hand back the defaults.
    pub fn load(path: impl AsRef<std::path::Path>) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, text + "\n")
    }

    /// Every switch with its current value, ready to render.
    pub fn describe(&self) -> Vec<Setting> {
        let map = self.as_map();
        SCHEMA
            .iter()
            .map(|(key, section, label, detail, restart)| Setting {
                key,
                section,
                label,
                detail,
                kind: kind_of(key),
                value: map.get(*key).cloned().unwrap_or(serde_json::Value::Null),
                restart: *restart,
            })
            .collect()
    }

    /// Sets one switch by key. Returns false for an unknown key or a value of
    /// the wrong shape, leaving the settings untouched — a client sending
    /// nonsense gets a no-op, never a corrupted file.
    pub fn set(&mut self, key: &str, value: serde_json::Value) -> bool {
        if !SCHEMA.iter().any(|(k, ..)| *k == key) {
            return false;
        }
        let mut map = self.as_map();
        map.insert(key.to_string(), value);
        match serde_json::from_value(serde_json::Value::Object(map)) {
            Ok(updated) => {
                *self = updated;
                true
            }
            Err(_) => false,
        }
    }

    fn as_map(&self) -> serde_json::Map<String, serde_json::Value> {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        }
    }

    /// Which day a week starts on, for the calendar grid.
    pub fn week_start(&self) -> chrono::Weekday {
        if self.week_starts_monday {
            chrono::Weekday::Mon
        } else {
            chrono::Weekday::Sun
        }
    }
}

fn kind_of(key: &str) -> SettingKind {
    match key {
        "theme" => SettingKind::Choice {
            options: vec![
                ("system".into(), "System".into()),
                ("dark".into(), "Dark".into()),
                ("light".into(), "Light".into()),
            ],
        },
        "vault_path" => SettingKind::Path { placeholder: "Beside the database".into() },
        _ => SettingKind::Toggle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_switch_in_the_schema_is_a_real_field() {
        let map = Settings::default().as_map();
        for (key, ..) in SCHEMA {
            assert!(map.contains_key(*key), "{key} is described but is not a field");
        }
    }

    /// The other direction, which is the one that rots: a field added without a
    /// schema entry would be invisible on the settings page forever.
    #[test]
    fn every_field_is_described() {
        let map = Settings::default().as_map();
        for key in map.keys() {
            assert!(
                SCHEMA.iter().any(|(k, ..)| k == key),
                "{key} is a field but no settings page would show it"
            );
        }
    }

    #[test]
    fn set_rejects_nonsense_without_damage() {
        let mut settings = Settings::default();
        assert!(!settings.set("nope", serde_json::json!(true)));
        assert!(!settings.set("reminders", serde_json::json!("yes please")));
        assert_eq!(settings, Settings::default());

        assert!(settings.set("reminders", serde_json::json!(false)));
        assert!(!settings.reminders);
    }

    #[test]
    fn a_partial_file_keeps_the_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"reminders": false}"#).unwrap();
        assert!(!parsed.reminders);
        assert!(parsed.calendar_page, "an unmentioned switch keeps its default");
    }

    #[test]
    fn a_broken_file_still_starts_the_app() {
        let dir = std::env::temp_dir().join("mtodo-settings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("broken.json");
        std::fs::write(&path, "{ not json at all").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_saved_file_reads_back_identical() {
        let dir = std::env::temp_dir().join("mtodo-settings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.json");
        let mut settings = Settings::default();
        settings.set("theme", serde_json::json!("dark"));
        settings.set("vault_path", serde_json::json!("D:/notes"));
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        let _ = std::fs::remove_file(&path);
    }
}
