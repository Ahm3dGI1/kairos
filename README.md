# Kairos

*Kairos* means the opportune moment: which thing is worth doing now?

An offline-first Windows todo app. Type `gym every day 5pm @health #fitness !p1`
to capture a structured recurring task. Your data lives in editable Markdown
files, with SQLite as a local index. No account or paid tier.

## Features

- Tasks grouped by overdue, today, tomorrow, next seven days, later and someday.
- Natural-language dates, times, priorities, projects, tags and custom weekday recurrence.
- Task notes, subtasks, recurring backlogs, per-occurrence moves and skips.
- Calendar, monthly checkbox/numeric/duration habits, and a monthly journal.
- Linked task/habit completion with undo and redo.
- Workout routines, exercises, dated sessions, additional sets and older-session browsing.
- Tray, reminders, global quick add, themes and optional Windows startup.
- Independent widgets for every view, with saved positions, sizes, filters and pin states.

## Widgets and startup

1. Open a view and select **Open as widget**, or create widgets from Settings.
2. Drag the widget header, resize its edges, and optionally select **Pin**.
3. Choose **Save layout** in a widget or **Save current widget layout** in Settings.
4. Keep **Restore saved layout when Kairos starts** enabled.
5. Enable **Start with Windows** from the installed app to restore the layout at sign-in.

Closing a widget does not change the saved layout. Close unwanted widgets and save again
to update it. Layouts live in `widget-layout.json` in the vault. If a monitor is missing,
widgets reopen on an available monitor. Pinning keeps a widget above other windows;
unpinned widgets behave like normal windows.

## Data and recovery

The default vault is `%USERPROFILE%\Kairos`; an existing `Master Todo` vault is adopted.
Settings shows the actual location.

```text
tasks/inbox.md       tasks without a project
habits/habits.md     habit definitions
habits/2026-09.md    daily values and monthly journal
workouts/push.md     a routine and its sessions
settings.json        preferences
widget-layout.json  saved widget windows
```

The vault's generated README explains the text formats. Preserve IDs when editing existing
records. External edits are detected by file contents, and app edits check for conflicts
before writing. Unreadable files block import rather than silently removing their data.
A conflicting or failed save is reported in the app.

Exports stage their output and replace individual files atomically. The five latest changed
exports keep original Markdown in `.kairos-backups` inside the vault. Database snapshots
are also kept under `%APPDATA%\dev.kairos.desktop\backups` before imports.
A multi-file export is not a filesystem-wide atomic transaction: recovery copies cover an
interruption midway through it. Back up the vault independently for long-term history.

To recover, quit Kairos, copy the desired backup files into the matching vault folders, and
reopen it. Undo history is database-only; external edits invalidate history for affected tasks.
Turning the vault off makes SQLite the only active data store.

## Build and test

Requires Windows, Rust 1.88+ with the MSVC toolchain, Visual Studio C++ build tools,
and WebView2. The app frontend uses plain HTML/CSS/ES modules, with no bundler.

```sh
cargo run -p kairos-windows
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo install tauri-cli --version "^2" --locked
cargo tauri build
```

Installers are written under `target/release/bundle/nsis` and `target/release/bundle/msi`.
Unsigned installers may trigger Windows security prompts. Only install builds you trust.
The app uses Windows WebView2 rather than shipping a browser engine beside its executable.

Headless frontend tests use Node and Playwright as optional development tools; neither is
needed to build or run the app. See [the Windows README](windows-app/README.md).
CI runs Rust tests, formatting, lint checks and the frontend regressions on Windows.

## Repository

- `core/`: task semantics, parsing, recurrence, vault, SQLite index and layout models.
- `windows-app/`: Tauri integration, static frontend and browser tests.

Linux, mobile and a self-hosted sync backend are future work. Concurrent edits in a synced
folder can conflict; this release does not provide multi-device merge semantics.

## License

[MIT](LICENSE), Copyright 2026 Ahmed Ibrahim.
