# Kairos Windows app

Tauri v2 with static HTML, CSS and ES modules. `/core` owns task and storage semantics.
The application has no npm dependencies or frontend build step.

## Development

```sh
cargo run -p kairos-windows
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Use Rust 1.88+ with MSVC, the Visual Studio C++ build tools, and WebView2.
Changes to embedded frontend assets require rebuilding the app.

## Installers

```sh
cargo install tauri-cli --version "^2" --locked
cargo tauri build
```

Artifacts appear under `target/release`:

- `bundle/nsis/Kairos_<version>_x64-setup.exe`
- `bundle/msi/Kairos_<version>_x64_en-US.msi`
- `kairos-windows.exe`

Keep the workspace and Tauri configuration versions aligned. Build installers from the
commit being released; old installers do not pick up source changes automatically.
Enable Start with Windows from the installed app, since the startup entry uses the path
of the executable registering it. No server is needed to run Kairos.

## Headless frontend tests

Tests mock the Tauri bridge, exercise the real DOM, and use temporary browser profiles.
They do not launch Kairos or access the user's vault.

With Node and Playwright available:

```sh
node --test windows-app/tests/frontend.test.cjs
```

Set `KAIROS_PLAYWRIGHT` to the installed Playwright package path when it is outside the
normal module search path. Optionally set `KAIROS_BROWSER_CHANNEL=msedge` to use an installed
Edge browser; otherwise install Playwright Chromium. CI provisions these tools separately
from the application.

## Widgets

Every view can be opened as an independent widget. The window header supports dragging,
pinning, closing and saving the layout. Settings manages startup restoration and creates
widgets for Tasks, Calendar, Habits, Workout and Settings.

Saving records all open view widgets, including their selected task list or workout routine.
Positions use physical monitor coordinates; sizes use logical pixels. Windows whose saved
position is no longer visible are moved onto an available monitor at restoration.
The original Today sticky note remains available independently.

## Keyboard

| Key | Action |
| --- | --- |
| N | Capture |
| / | Search |
| J / K or arrows | Move task selection |
| Space / X | Complete selected task |
| Enter | Open task details |
| Del | Delete selected task |
| U / Ctrl+Z | Undo task action |
| Y / Ctrl+Shift+Z | Redo task action |
| H / L | Previous / next month |
| S | Start workout session |
| W | Toggle Today sticky note |
| 1-5 | Switch views |
| Esc | Close the current editor or panel |
| Ctrl+Shift+Space | Global quick add |

## Source map

- `src/app.js`: view navigation and task/calendar rendering.
- `src/capture.js`, `detail.js`, `habits.js`, `workout.js`, `settings.js`: view logic.
- `src/shared.js`: bridge, errors, DOM helpers and themes.
- `src-tauri/src/commands/`: thin core command wrappers.
- `src-tauri/src/state.rs`, `watcher.rs`: serialized file imports and exports.
- `src-tauri/src/widgets.rs`: widget windows and saved layouts.
- `src-tauri/src/shell.rs`, `reminders.rs`: tray, shortcuts, startup and notifications.

Storage, recovery and startup instructions are in the [root README](../README.md).
