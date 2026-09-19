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
  Each recognized phrase gets a tinted pill in the field itself, and **backspace
  against a pill un-parses it** — the words drop back into the title, the way an
  editor undoes an autoformat. Capture only appears on the Tasks and Calendar
  pages; the habit month has nothing to capture into.
- **Global hotkey** `Ctrl+Shift+Space` — a Spotlight-style overlay from anywhere,
  with the same live preview. Enter adds and dismisses; `Shift+Enter` keeps it
  open for a run of captures; Escape or losing focus dismisses it.
- **System tray** — hover to preview what is due today, click to open; the menu
  has quick add, the widget toggle and quit. Closing a window parks the app in
  the tray rather than quitting: a todo app that disappears when you close its
  window stops reminding you.
- **Sticky note** — a small panel of today's tasks. By default it behaves like
  any other window and drops behind whatever you focus next; the pin in its
  header makes it stay above everything. It never takes focus when it appears.
- **Lists** — a second sidebar inside Tasks holding every project in use and
  every tag, so a task's project reads as the list it belongs to.
- **One task list**, not a row of tabs: everything ahead in a single scroll,
  grouped Overdue → Today → Tomorrow → Next 7 days → Later → Someday. Completed
  work is a toggle rather than a destination. Projects and tags narrow the list
  from the sidebar.
- **A minimal detail pane** — a date chip (opening a calendar with time and
  repeat), a priority flag, the title, and one body that switches between notes
  and subtasks. Everything else stays folded away until asked for.
- **Calendar** — a month grid with recurrence expanded, so a daily task appears
  on every day it actually falls on.
- **Habits by the month** — habits down the side, days across the top, one tick
  per cell, with screen time and sleep as their own numeric rows. Beside it, a
  journal for the whole month: entries carry their own headings, so a day, a
  week or a trip can each be one block.
- **Search** across titles, notes, subtasks, tags and projects.
- **Undo and redo** for completions, deletions and edits, both backed by logs in
  the database, so they survive a restart. A fresh edit discards the redo stack,
  because replaying onto a world that has moved on is not the same act.
- **A backlog checklist** — a recurring task can read its subtask list as a
  backlog rather than a set of steps. "Learning" repeats weekly and holds the
  things to get to; ticking one finishes that week's occurrence and moves the
  series on, leaving the rest as what is left rather than work outstanding.
- **The workout book** — routines (push, pull, legs) hold exercises, and each
  session is a dated column of reps and weights beside them. Starting a session
  copies the last one's numbers so you adjust rather than retype.
- **Recurring exceptions** — skip or push a single occurrence from the detail
  pane without breaking the series.
- **Reminders** — a Windows notification when a task's time arrives. The loop
  polls rather than scheduling a timer per task: tasks move, recurrence shifts
  and machines sleep, and a poll survives all three where scheduled timers go
  stale. It only fires for a time that passed in the last five minutes, so
  reopening the app in the evening does not replay the whole day, and only for
  an occurrence that actually falls today, so overdue work does not ring again
  every day at its old time.

  Run from `cargo run`, Windows attributes the toast to whatever launched the
  binary, because an unpackaged app has no registered identity of its own. An
  installed build (`cargo tauri build`) shows up as Master Todo.

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
| `Y` or `Ctrl+Shift+Z` | redo |
| `S` | start today's session (Workout) |
| `W` | toggle the sticky note |
| `1`–`4` | Tasks, Calendar, Habits, Workout |
| `Esc` | close the pane, clear the search, dismiss the overlay |
| `Ctrl+Shift+Space` | quick-add overlay, from anywhere in Windows |

## Layout

```
src/                 frontend — no build step
  index.html/app.js    main window: pages, task list, calendar, wiring
  capture.js           the capture field: parse pills and backspace-to-revert
  detail.js            the detail pane, its date and priority pickers
  habits.js            the habit month grid and the month journal
  workout.js           routines, exercises and the session grid
  quick-add.html/.js   the hotkey overlay, sharing capture.js
  widget.html/.js      the sticky note
  shared.js            IPC + date formatting shared by all of them
  styles.css           tokens, light and dark
  overlay.css          the two frameless windows
src-tauri/
  src/main.rs          builder, plugins, the IPC surface
  src/commands.rs      commands — thin wrappers over the core
  src/workout_commands.rs  the workout book
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

- Recurrence rules beyond the repeat picker's seven options — a rule the picker
  cannot express is shown rather than silently reset, but must be re-typed to
  change
- Per-task reminder offsets ("15 minutes before"); reminders fire at the due
  time itself
- Drag-and-drop reordering, and manual sort orders
- Sync (waiting on [`/sync-server`](../sync-server))
