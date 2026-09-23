use crate::state::AppState;
use kairos_core::widgets::{WidgetLayout, WidgetSpec, WidgetView};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct Windows(pub Mutex<HashMap<String, WidgetSpec>>);

fn path(state: &AppState) -> std::path::PathBuf {
    state.paths.vault_dir.join("widget-layout.json")
}

fn open(app: &AppHandle, mut spec: WidgetSpec) -> Result<(), String> {
    spec.normalize();
    let label = format!("widget-{}", spec.id);
    if app.get_webview_window(&label).is_some() {
        return Ok(());
    }
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let visible = monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        spec.x >= p.x
            && spec.y >= p.y
            && (spec.x as i64) < p.x as i64 + s.width as i64 - 80
            && (spec.y as i64) < p.y as i64 + s.height as i64 - 40
    });
    if !visible {
        if let Some(m) = monitors.first() {
            spec.x = m.position().x + 40;
            spec.y = m.position().y + 40;
        }
    }
    app.state::<Windows>()
        .0
        .lock()
        .map_err(|_| "widget lock poisoned")?
        .insert(label.clone(), spec.clone());
    let result =
        WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html?widget=1".into()))
            .title(format!("Kairos - {}", spec.view.name()))
            .inner_size(spec.width, spec.height)
            .min_inner_size(360.0, 300.0)
            .decorations(false)
            .resizable(true)
            .always_on_top(spec.pinned)
            .focused(false)
            .visible(false)
            .build();
    let window = match result {
        Ok(w) => w,
        Err(e) => {
            app.state::<Windows>().0.lock().map_err(|_| "widget lock poisoned")?.remove(&label);
            return Err(e.to_string());
        }
    };
    window.set_position(PhysicalPosition::new(spec.x, spec.y)).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn widget_context(
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<Option<WidgetSpec>, String> {
    Ok(app
        .state::<Windows>()
        .0
        .lock()
        .map_err(|_| "widget lock poisoned")?
        .get(window.label())
        .cloned())
}

#[tauri::command]
pub fn create_widget(
    app: AppHandle,
    view: WidgetView,
    project: Option<String>,
    routine: Option<uuid::Uuid>,
) -> Result<(), String> {
    let offset =
        app.state::<Windows>().0.lock().map_err(|_| "widget lock poisoned")?.len() as i32 * 24;
    open(
        &app,
        WidgetSpec {
            id: uuid::Uuid::new_v4(),
            view,
            project,
            routine,
            x: 80 + offset,
            y: 80 + offset,
            width: 650.0,
            height: 520.0,
            pinned: false,
        },
    )
}

#[tauri::command]
pub fn update_widget_context(
    window: tauri::WebviewWindow,
    app: AppHandle,
    view: WidgetView,
    project: Option<String>,
    routine: Option<uuid::Uuid>,
) -> Result<(), String> {
    if let Some(spec) =
        app.state::<Windows>().0.lock().map_err(|_| "widget lock poisoned")?.get_mut(window.label())
    {
        spec.view = view;
        spec.project = project;
        spec.routine = routine;
    }
    Ok(())
}

#[tauri::command]
pub fn pin_widget(window: tauri::WebviewWindow, app: AppHandle) -> Result<bool, String> {
    let windows = app.state::<Windows>();
    let mut entries = windows.0.lock().map_err(|_| "widget lock poisoned")?;
    let spec = entries.get_mut(window.label()).ok_or("Not a view widget")?;
    let pinned = !spec.pinned;
    window.set_always_on_top(pinned).map_err(|e| e.to_string())?;
    spec.pinned = pinned;
    Ok(pinned)
}

#[tauri::command]
pub fn widget_layout(state: State<'_, AppState>) -> Result<WidgetLayout, String> {
    WidgetLayout::load(&path(&state)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_widget_layout(
    app: AppHandle,
    state: State<'_, AppState>,
    restore_on_start: bool,
) -> Result<WidgetLayout, String> {
    let _guard = state.io.lock().map_err(|_| "storage lock poisoned")?;
    let mut layout = WidgetLayout { restore_on_start, widgets: Vec::new() };
    for (label, original) in
        app.state::<Windows>().0.lock().map_err(|_| "widget lock poisoned")?.iter()
    {
        if let Some(window) = app.get_webview_window(label) {
            let mut spec = original.clone();
            let pos = window.outer_position().map_err(|e| e.to_string())?;
            let size = window.inner_size().map_err(|e| e.to_string())?;
            let scale = window.scale_factor().map_err(|e| e.to_string())?;
            spec.x = pos.x;
            spec.y = pos.y;
            spec.width = size.width as f64 / scale;
            spec.height = size.height as f64 / scale;
            layout.widgets.push(spec);
        }
    }
    layout.widgets.sort_by_key(|w| w.id);
    layout.save(&path(&state)).map_err(|e| e.to_string())?;
    Ok(layout)
}

#[tauri::command]
pub fn set_widget_restore(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let _guard = state.io.lock().map_err(|_| "storage lock poisoned")?;
    let mut layout = WidgetLayout::load(&path(&state)).map_err(|e| e.to_string())?;
    layout.restore_on_start = enabled;
    layout.save(&path(&state)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_widget_layout(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let layout = WidgetLayout::load(&path(&state)).map_err(|e| e.to_string())?;
    for spec in layout.widgets {
        open(&app, spec)?;
    }
    Ok(())
}

pub fn startup(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let layout = WidgetLayout::load(&path(&state)).map_err(|e| e.to_string())?;
    if layout.restore_on_start {
        for spec in layout.widgets {
            open(app, spec)?;
        }
    }
    Ok(())
}

pub fn closed(app: &AppHandle, label: &str) {
    if let Ok(mut entries) = app.state::<Windows>().0.lock() {
        entries.remove(label);
    }
}
