use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WidgetView {
    Agenda,
    Calendar,
    Habits,
    Workout,
    Settings,
}

impl WidgetView {
    pub fn name(self) -> &'static str {
        match self {
            Self::Agenda => "Tasks",
            Self::Calendar => "Calendar",
            Self::Habits => "Habits",
            Self::Workout => "Workout",
            Self::Settings => "Settings",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Agenda => "agenda",
            Self::Calendar => "calendar",
            Self::Habits => "habits",
            Self::Workout => "workout",
            Self::Settings => "settings",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetSpec {
    pub id: uuid::Uuid,
    pub view: WidgetView,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub routine: Option<uuid::Uuid>,
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub pinned: bool,
}

impl WidgetSpec {
    pub fn normalize(&mut self) {
        self.width = if self.width.is_finite() { self.width.clamp(360.0, 3840.0) } else { 600.0 };
        self.height =
            if self.height.is_finite() { self.height.clamp(300.0, 2160.0) } else { 500.0 };
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetLayout {
    pub restore_on_start: bool,
    pub widgets: Vec<WidgetSpec>,
}

impl Default for WidgetLayout {
    fn default() -> Self {
        Self { restore_on_start: true, widgets: Vec::new() }
    }
}

impl WidgetLayout {
    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut layout: Self = serde_json::from_slice(&bytes)?;
                for spec in &mut layout.widgets {
                    spec.normalize();
                }
                let mut ids = std::collections::HashSet::new();
                layout.widgets.retain(|w| ids.insert(w.id));
                Ok(layout)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        crate::io::atomic_write(path, &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}
