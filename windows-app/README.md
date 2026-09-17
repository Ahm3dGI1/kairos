# /windows-app

The Windows 11 client — phase 1 of the project. A Tauri v2 shell over
[`/core`](../core), which owns every decision about what a task means.

## Running it

```sh
cargo run -p mtodo-windows
```

That is the whole dev loop. There is **no bundler and no npm**: the frontend is
plain HTML, CSS and ES modules under `src/`, and `withGlobalTauri` exposes the
IPC bridge as `window.__TAURI__`, so the repo carries no `node_modules` and the
frontend needs no build step. Editing a file under `src/` needs a rebuild only
because Tauri embeds the assets into the binary.

Packaging an installer (`.msi`/`.exe`) does need the Tauri CLI:

```sh
cargo install tauri-cli --version "^2"
cargo tauri build
```

Requires Rust 1.88+ (current Tauri dependencies), the MSVC toolchain, and the
WebView2 runtime — present on Windows 11 by default.

## What it does

- **Single-line capture** with a live parse preview: type
  `gym every day 5pm @health #fitness !p1` and see the title, date, time,
  recurrence, project, tags and priority resolve as you type, before committing.
- **Global hotkey** `Ctrl+Shift+Space` — a Spotlight-style overlay from anywhere,
  with the same live preview. Enter adds and dismisses; `Shift+Enter` keeps it
  open for a run of captures; Escape or losing focus dismisses it.
- **System tray** — open, quick add, toggle the widget, quit. Closing a window
  parks the app in the tray rather than quitting: a todo app that disappears
  when you close its window stops reminding you.
- **Desktop widget** — a small always-on-top panel of today's tasks. It does not
  take focus when it appears, so it never interrupts what you were doing.
- **Views** — Today, Next 7 days, All, Overdue, Inbox, Completed, plus every
  project and tag in use, each with a live count.
- **Calendar** — a month grid with recurrence expanded, so a daily task appears
  on every day it actually falls on.
- **Search** across titles, notes, subtasks, tags and projects.
- **Undo** for completions, deletions and edits, backed by the store's log, so
  it survives a restart.
- **Recurring exceptions** — skip or push a single occurrence from the detail
  pane without breaking the series.

## Keyboard

The app is keyboard-first; the mouse is optional everywhere.

| Key | Action |
| --- | --- |
| `N` | focus the capture field |
| `/` | focus search |
| `J` / `K` or arrows | move the selection |
| `Space` or `X` | complete |
| `Enter` | open the detail pane |
| `E` | edit the selected task |
| `Del` | delete |
| `U` or `Ctrl+Z` | undo |
| `C` | list ⇄ calendar |
| `W` | toggle the desktop widget |
| `1`–`6` | jump to a view |
| `Esc` | close the pane, clear the search, dismiss the overlay |
| `Ctrl+Shift+Space` | quick-add overlay, from anywhere in Windows |

## Layout

```
src/                 frontend — no build step
  index.html/app.js    main window: views, list, calendar, detail pane
  quick-add.html/.js   the hotkey overlay
  widget.html/.js      the desktop widget
  shared.js            IPC + date formatting shared by all three
  styles.css           tokens, light and dark
  overlay.css          the two frameless windows
src-tauri/
  src/main.rs          builder, plugins, the IPC surface
  src/commands.rs      commands — thin wrappers over the core
  src/shell.rs         tray, global hotkey, window management
  src/state.rs         the shared store, and the tasks-changed event
  tauri.conf.json      windows, CSP, bundle
  capabilities/        Tauri v2 permissions
```

All three windows share one open `Store` and re-read on a `tasks-changed`
event, so completing something in the widget updates the main list instantly.

## Data

`%APPDATA%\dev.mastertodo.desktop\tasks.db` — SQLite, local-first, no network.
Deleting that file resets the app.

## Not yet

- Reminders and notifications (the plugin is wired up; nothing schedules yet)
- Editing a recurrence rule from the UI — the parser sets it, the detail pane
  cannot yet change it
- Drag-and-drop reordering, and manual sort orders
- Sync (waiting on [`/sync-server`](../sync-server))
