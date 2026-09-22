# Kairos

*καιρός* — the opportune moment, as against *chronos*, clock time. The useful
question is not what time it is but which thing is worth doing now.

A todo app for Windows 11 whose defining interaction is one line of plain
English. Type

```
gym every day 5pm @health #fitness !p1
```

and you get a task called "Gym" that repeats daily at 17:00, in the Health
project, tagged fitness, at top priority. No date picker, no time picker, no
recurrence dialog.

Free, open source, no paid tier, and no account. Your data is a folder of
Markdown files you own.

## Why

Every capable todo app either paywalls the features that make it capable,
locks your data in someone else's cloud, or makes you fill in a form to record
a thought you could have typed in four words. Kairos is the alternative to all
three.

## What it does

- **Single-line capture** with a live preview of what was understood. Each
  recognised phrase is tinted in the field, and backspacing against one
  un-parses it — the words drop back into the title, the way an editor undoes
  an autoformat.
- **One task list**, grouped Overdue → Today → Tomorrow → Next 7 days → Later
  → Someday. Projects and tags narrow it from a sidebar.
- **Recurring tasks** with per-occurrence exceptions: skip or push a single
  Monday without breaking the series.
- **A calendar** with recurrence expanded onto every day a task falls on.
- **Habits by the month** — a tick, a duration, or a count with a unit. Link a
  habit to a task and completing the task records the habit.
- **A workout book** — routines hold exercises, and each session is a dated
  column of reps and weights. Starting a session copies last time's numbers.
- **Windows-native touches** — a tray icon with a hover preview, a sticky note
  that can pin above other windows, `Ctrl+Shift+Space` quick add from
  anywhere, reminders, and start-with-Windows.
- **Undo and redo** that survive a restart, because the log is in the database.

Full detail, including the keyboard map: [`windows-app/README.md`](windows-app/README.md).

## Your data is a folder of files

This is the part that matters. Everything lives in a vault of plain text —
by default `%USERPROFILE%\Kairos`:

```
tasks/inbox.md     one file per project, one checklist item per task
habits/2026-09.md  a month of ticks, numbers and journal
workouts/push.md   a routine, its exercises, and every session
settings.json      every switch on the Settings page
```

A task is an ordinary Markdown checklist item, so Obsidian renders a project
file as a task list:

```markdown
- [ ] Gym due:2026-09-22 at:17:00 repeat:"every day" #fitness !p1 ^7f3a9c2e-…
  - [ ] warm up
  > Bring the new shoes.
```

**These files are the record.** The SQLite database is an index rebuilt from
them — delete it and the next launch restores everything. Edit them in any
editor, or point an AI agent at the folder, and the app picks the change up in
about a second. Writing a line with no `^id` runs it through the same parser
as the capture bar, so

```markdown
- [ ] gym every day 5pm #health
```

typed into a file becomes exactly the task typing it into the app would.

Putting the vault in a synced folder or a git repo is, for now, most of what
sync would give you.

## Install

Download the setup from [Releases](../../releases) and run it. The build is
unsigned, so Windows will show "Windows protected your PC" the first time —
More info → Run anyway.

It installs as a single 12 MB executable. Rust links statically, the frontend
is embedded in the binary, and the web view is the WebView2 runtime Windows 11
already ships, so there is no runtime to lay down beside it.

## Build it yourself

```sh
cargo run -p kairos-windows      # the whole dev loop
cargo test --workspace
cargo tauri build                # installers -> target/release/bundle
```

Needs Rust 1.88+ and the MSVC toolchain. There is **no npm and no bundler** —
the frontend is plain HTML, CSS and ES modules, so `cargo run` is all of it.

## Layout

```
/core          task model, parsing, recurrence, the vault, the store
/windows-app   the Tauri shell: tray, widget, hotkey, calendar, UI
```

Linux and mobile clients and a self-hostable sync server are planned. They get
directories when they get code.

## Licence

MIT — see [LICENSE](LICENSE).
