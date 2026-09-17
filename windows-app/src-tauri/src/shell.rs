//! The Windows-specific surface: tray icon, global hotkey, and the two
//! auxiliary windows.
//!
//! These are the things that make the app feel native rather than like a
//! browser tab — spec §4, "Windows-specific UX". Nothing here knows what a task
//! is; it only decides which window the user is looking at.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// The Spotlight-style launcher key. Ctrl+Shift+Space is unclaimed by Windows
/// itself and by the common editors, which is the whole requirement.
const QUICK_ADD_HOTKEY: (Modifiers, Code) =
    (Modifiers::CONTROL.union(Modifiers::SHIFT), Code::Space);

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    build_auxiliary_windows(app)?;
    build_tray(app)?;
    register_hotkey(app)?;
    Ok(())
}

/// Creates the quick-add overlay and the desktop widget up front, both hidden.
///
/// Building them at startup rather than on demand is what makes the hotkey feel
/// instant: showing an existing window is immediate, where constructing a
/// webview is not.
fn build_auxiliary_windows(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    WebviewWindowBuilder::new(app, "quick-add", WebviewUrl::App("quick-add.html".into()))
        .title("Quick add")
        .inner_size(680.0, 132.0)
        .decorations(false)
        .transparent(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .center()
        .visible(false)
        .build()?;

    WebviewWindowBuilder::new(app, "widget", WebviewUrl::App("widget.html".into()))
        .title("Master Todo widget")
        .inner_size(320.0, 420.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(true)
        .focused(false)
        .visible(false)
        .build()?;

    Ok(())
}

fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let open = MenuItem::with_id(app, "open", "Open Master Todo", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "Quick add\tCtrl+Shift+Space", true, None::<&str>)?;
    let widget = MenuItem::with_id(app, "widget", "Toggle desktop widget", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quick, &widget, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("tray")
        .menu(&menu)
        .tooltip("Master Todo")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quick" => toggle_quick_add(app),
            "widget" => toggle_widget(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // A left click opens the app; the menu stays on right click.
            if let TrayIconEvent::Click { button, .. } = event {
                if button == tauri::tray::MouseButton::Left {
                    show_main(tray.app_handle());
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn register_hotkey(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let (modifiers, code) = QUICK_ADD_HOTKEY;
    let shortcut = Shortcut::new(Some(modifiers), code);

    app.global_shortcut().on_shortcut(shortcut, move |app, _shortcut, event| {
        // Fire on press only; the release would immediately toggle it back.
        if event.state() == ShortcutState::Pressed {
            toggle_quick_add(app);
        }
    })?;
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Shows the quick-add overlay, or hides it if it already has the screen.
pub fn toggle_quick_add(app: &AppHandle) {
    let Some(window) = app.get_webview_window("quick-add") else { return };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    // Re-centre on each open: the user may have moved to another monitor.
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.emit_to("quick-add", "quick-add-opened", ());
}

pub fn toggle_widget(app: &AppHandle) {
    let Some(window) = app.get_webview_window("widget") else { return };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let _ = window.show();

    // The widget is for glancing at, not working in, so it must not take the
    // keyboard from whatever the user was doing. Showing a window focuses it on
    // Windows, so hand focus straight back to the main window when that is
    // where the user was.
    if let Some(main) = app.get_webview_window("main") {
        if main.is_visible().unwrap_or(false) {
            let _ = main.set_focus();
        }
    }
}

/// Hides a window rather than destroying it — used by the overlay's Escape key
/// and the main window's close button, so the app keeps living in the tray.
#[tauri::command]
pub fn hide_window(app: AppHandle, label: String) {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn toggle_widget_command(app: AppHandle) {
    toggle_widget(&app);
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    show_main(&app);
}
