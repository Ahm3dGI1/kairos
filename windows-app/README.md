# /windows-app

The Windows 11 client — phase 1 of the project. A Tauri v2 shell over
[`/core`](../core), which owns every decision about what a task means.

## Running it

```sh
cargo run -p kairos-windows
```

That is the whole dev loop. There is **no bundler and no npm**: the frontend is
plain HTML, CSS and ES modules under `src/`, and `withGlobalTauri` exposes the
IPC bridge as `window.__TAURI__`, so the repo carries no `node_modules` and the
frontend needs no build step. Editing a file under `src/` needs a rebuild only
because Tauri embeds the assets into the binary.

Requires Rust 1.88+ (current Tauri dependencies), the MSVC toolchain, and the
WebView2 runtime — present on Windows 11 by default.

## Building a release

```sh
cargo install tauri-cli --version "^2" --locked   # once
cargo tauri build
```

The first build downloads WiX and NSIS itself; no manual installer toolchain is
needed. It writes three things under `target/release/`:

| Artifact | Path | Use |
| --- | --- | --- |
| Setup | `bundle/nsis/Kairos_<version>_x64-setup.exe` | what a person downloads and runs |
| MSI | `bundle/msi/Kairos_<version>_x64_en-US.msi` | silent/managed install (`msiexec /i … /qn`) |
| Bare binary | `kairos-windows.exe` | portable — runs with no install |

Both installers register the app so reminders are attributed to Kairos
rather than to whatever launched it, and both add the tray icon and Start menu
entry. The bare `.exe` works too, but Windows will not know its identity.

The whole app is that one file. Rust links statically, the frontend is
embedded into the binary at compile time, and the web view is the WebView2
runtime Windows 11 already ships — so there is no runtime to lay down beside
it. An Electron app has to bring its own copy of Chromium and Node, which is
why one installs as a folder of a hundred-odd files and this installs as an
executable. Turn **Start with Windows** on and enable it from the *installed*
build: the startup entry records the path of whichever binary registered it,
so doing it from `cargo run` would point Windows at `target/debug`.

The version lives in **two** places that must agree: `version` in the workspace
`Cargo.toml` and `version` in `src-tauri/tauri.conf.json`. Bump both, or the
installer's version and the binary's disagree.

Installing over an older version keeps the database — it lives in `%APPDATA%`,
not in the install directory (see [Data](#data)), so upgrades never touch it.

### The SmartScreen warning

The build is unsigned, so the first run of the installer shows "Windows
protected your PC" — More info → Run anyway. That is expected, and it is what
anyone else downloading it will see too. Removing it means an
Authenticode certificate from a CA (an OV cert costs a few hundred a year and
still needs reputation to build; an EV cert clears SmartScreen immediately and
costs more). Tauri signs automatically once `windows.signCommand` or the
`TAURI_SIGNING_*` environment is configured. For a self-hosted, self-installed
app this is an optional expense, not a prerequisite.

## Distributing it

There is no server: the app is entirely local, so "hosting" means hosting the
*download*. Push the repo to GitHub and attach the installer to a release:

```sh
gh release create v0.1.0 \
  "target/release/bundle/nsis/Kairos_0.1.0_x64-setup.exe" \
  "target/release/bundle/msi/Kairos_0.1.0_x64_en-US.msi" \
  --title "Kairos 0.1.0" --notes "First release."
```

Releases are free for public repos, need no infrastructure, and give a stable
download URL. A sync server is the only part of the project that will ever
need somewhere to run, and it is not built — until then, every device is
independent, and a vault in a synced folder covers most of what sync would.

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
- **Habits by the month** — habits down the side, days across the top. A habit
  is a tick, a duration, or a count with a unit; a numeric row keeps the same
  grid and shows its value as an intensity, read exactly on hover and typed
  into an input floated over the cell. Beside it, a journal for the whole
  month: entries carry their own headings, so a day, a week or a trip can each
  be one block.
- **A task can be a habit.** "Gym" is both a thing to do five times a week and
  a thing to have a run of; link them from the detail pane and completing the
  task ticks the habit for that day.
- **Editing a habit** — click its name: rename it, change what it records and
  in what unit, archive it or delete it. Changing what it records leaves the
  numbers already entered alone. Archiving takes the row off the grid and
  keeps the history, and archived habits sit under the grid so there is a way
  back; deleting takes the ticks with it.
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
- **Repeats** — seven presets, and a row of day keys for anything else. The
  keys compose the phrase the capture bar would have parsed rather than being
  a second way to say a rule, and a preset lights up the days it means, so
  "every weekday" opens with Monday to Friday already on and editing one
  starts from where it is. Days read M T W Th F St S, because Tuesday and
  Thursday share a letter and so do Saturday and Sunday. The rule shows in the
  detail pane; the list rows stay as date, priority, project and tags.
- **Recurring exceptions** — skip or push a single occurrence from the detail
  pane without breaking the series.
- **Start with Windows** — off by default. On, it adds a per-user entry to
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` pointing at the
  installed binary with `--autostart`, and that copy starts straight into the
  tray rather than opening a window: the point of starting at sign-in is that
  the hotkey and reminders work before you have opened anything. It will not
  start hidden if the tray icon is switched off, because that would be
  starting with no way back. `settings.json` is the record — an entry deleted
  by hand is put back at the next launch, and one left behind is cleared.
- **Settings** — a page of switches for every feature above: which pages appear
  in the rail, whether capture tints what it parsed, the tray icon, the global
  hotkey, reminders, the theme, which day a week starts on, and the vault. The
  list is defined in [`/core`](../core/src/settings.rs) so a Linux client would
  offer the same switches, and the page renders whatever the core describes.
  The few that cannot take effect until a restart say so on the row rather
  than pretending otherwise.
- **Reminders** — a Windows notification when a task's time arrives. The loop
  polls rather than scheduling a timer per task: tasks move, recurrence shifts
  and machines sleep, and a poll survives all three where scheduled timers go
  stale. It only fires for a time that passed in the last five minutes, so
  reopening the app in the evening does not replay the whole day, and only for
  an occurrence that actually falls today, so overdue work does not ring again
  every day at its old time.

  Run from `cargo run`, Windows attributes the toast to whatever launched the
  binary, because an unpackaged app has no registered identity of its own. An
  installed build (`cargo tauri build`) shows up as Kairos.

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
| `1`–`5` | Tasks, Calendar, Habits, Workout, Settings |
| `Esc` | close the pane, clear the search, dismiss the overlay |
| `Ctrl+Shift+Space` | quick-add overlay, from anywhere in Windows |

## The icon

[`src-tauri/icons/icon.svg`](src-tauri/icons/icon.svg) is the source; the PNGs and the `.ico`
beside it are rendered from that geometry. It is a sundial cut down to a dial, one
hand, and the moment it points at — not a two-handed clock, because a clock
tells you the time and this app is about the one moment worth acting on. The
dial is left open where the hand leaves it. The plate sits just off black so
the icon still separates from a dark taskbar, and the amber is the interface's
single accent doing the same job it does there.

## Layout

```
src/                 frontend — no build step
  index.html/app.js    main window: pages, task list, calendar, wiring
  capture.js           the capture field: parse pills and backspace-to-revert
  detail.js            the detail pane, its date and priority pickers
  habits.js            the habit month grid and the month journal
  settings.js          the settings page, rendered from the core's descriptions
  workout.js           routines, exercises and the session grid
  quick-add.html/.js   the hotkey overlay, sharing capture.js
  widget.html/.js      the sticky note
  shared.js            IPC + date formatting shared by all of them
  styles.css           tokens, light and dark
  overlay.css          the two frameless windows
src-tauri/
  src/main.rs          builder, plugins, the IPC surface
  src/commands/        thin wrappers over the core, one file per page
    mod.rs               what they share: the store lock, the task view
    tasks.rs             capture, edit, complete, undo
    daily.rs             habits and the month journal
    workout.rs           the workout book
    settings.rs          the switches, and the vault's controls
  src/shell.rs         tray, global hotkey, window management
  src/state.rs         the shared store, the vault, and the data-changed event
  src/watcher.rs       noticing that someone edited the vault
  src/reminders.rs     the due-time poll and the tray tooltip
  tauri.conf.json      windows, CSP, bundle
  capabilities/        Tauri v2 permissions
```

All three windows share one open `Store` and re-read on a `data-changed`
event, so completing something in the widget updates the main list instantly.

## Data

Everything lives in a **vault** of plain text files, by default
`%USERPROFILE%\Kairos\`:

```
README.md          what the formats are
settings.json      every switch on the Settings page
tasks/inbox.md     one file per project, one checklist item per task
habits/habits.md   the habits; 2026-09.md is a month of ticks and journal
workouts/push.md   a routine, its exercises, and every session
```

These files are the real data. `%APPDATA%\dev.kairos.desktop\tasks.db` is
an index built from them — delete it and the next launch rebuilds it. Edits
made in any editor are picked up within about a second and a half, and the app
mirrors its own changes straight back out, touching only the files whose
contents actually differ.

A task is an ordinary Markdown checklist item, so Obsidian renders a project
file as a task list:

```markdown
- [ ] Gym due:2026-09-20 at:17:00 repeat:"every day" #fitness !p1 ^7f3a9c2e-…
  - [ ] warm up
  > Bring the new shoes.
```

Two rules make it safe to type into by hand. Metadata is read as a **suffix**,
so a `#` or a `due:` in the middle of a title stays in the title. And a line
with **no `^id` is treated as new**, so it goes through the same
natural-language parser as the capture bar — writing

```markdown
- [ ] gym every day 5pm #health
```

into any file gives exactly the task that line would have made in the app.
Lines that already carry an id are read literally, so editing a title never
silently moves a date.

Before an import replaces the database, the previous contents are written to
`%APPDATA%\dev.kairos.desktop\backups\` as JSON, and the last five are
kept — the vault can be somewhere only half-present, like a cloud folder
mid-sync. Turning the vault off in Settings makes the database the only copy
again.

## Not yet

- Recurrence rules beyond the repeat picker's seven options — a rule the picker
  cannot express is shown rather than silently reset, but must be re-typed to
  change
- Per-task reminder offsets ("15 minutes before"); reminders fire at the due
  time itself
- Drag-and-drop reordering, and manual sort orders
- Sync between devices. Putting the vault in a synced folder or a git repo
  gets most of the way there in the meantime.
