# Master Todo App

## Why this project exists

Every capable todo app either paywalls the features that make it capable, locks your data in
someone else's cloud, or makes you fill out a form — date picker, time picker, recurrence
dialog — to record a thought you could have typed in four words. Master Todo App is the free,
open-source alternative: you type one line, "gym every day 5pm", and it becomes a fully
structured recurring task. It is offline-first by design, self-hostable end to end, and has no
paid tier. Phase 1 targets Windows 11; Linux, Android, and iPad follow later.

## What it does

A todo app whose defining interaction is single-line natural-language capture. Beyond capture, v1
covers priorities, tags, subtasks, projects/sections, recurring tasks with per-occurrence
exceptions, a calendar view, quick filters, search, and keyboard-first navigation — plus
Windows-native touches: a tray icon with hover preview, an always-visible desktop widget, and a
global hotkey launcher. Sync is an optional layer over a local SQLite store, served by a server
users host themselves.

**The full spec lives in [`docs/todo-app-spec.md`](docs/todo-app-spec.md) — read it before making
architectural decisions.** It carries the detail this file omits: the complete feature list, sync
and auth model, stretch goals, and the open questions still unsettled.

## Tech stack

Still partly undecided — do not treat a candidate as a commitment.

| Area          | Choice                                          | Status         |
| ------------- | ----------------------------------------------- | -------------- |
| Core logic    | Rust — `mtodo-core`, parser landed               | Decided        |
| Windows shell | Tauri — lighter than Electron                    | Decided        |
| Local storage | SQLite, per device                               | Decided        |
| Sync server   | Postgres + realtime, or a custom minimal server  | Candidate      |
| Linux client  | Rust TUI (ratatui) or the existing Go/Bubbletea  | **Open** (§6)  |
| Mobile client | React Native or Flutter                          | **Open** (§6)  |
| Auth (v1)     | None — shared API key per device                 | Decided        |

Rust won the core-language question: it is already Tauri's backend language, so the Windows shell
links it with no bridge, and it alone also serves a Linux TUI. Mobile pays an FFI layer (uniffi).

## Repo structure

Monorepo — one maintainer, one shared core, so a parsing fix lands everywhere in one PR. Split into
separate repos only if independent maintainers take over a platform. Only `/core` has code; every
other directory is a README stub.

```
/core          shared business logic: task model, NL parsing, sync client  (crate mtodo-core)
/windows-app   Tauri shell: tray icon, widget, hotkey launcher, UI  (phase 1)
/linux-app     terminal client                                       (future)
/mobile-app    React Native or Flutter client                        (future)
/sync-server   self-hostable sync backend
/docs          spec and project documentation
```

## Build / test / lint

`/core` is a Rust crate in the root Cargo workspace; every other package is still a stub.

```sh
cargo build                  # workspace
cargo test                   # unit + corpus + doc tests
cargo clippy --all-targets   # lint
cargo fmt                    # format
```

## Conventions

- **`/core` owns all task semantics.** Parsing, recurrence, and the task model live there and
  nowhere else. Clients are presentation and platform integration only.
- **Offline-first is a hard requirement, not a fallback.** A feature that stops working without
  a network is a bug; sync reconciles later and is never in the critical path.
- **Self-hosting is the deployment model.** No dependency on a service the project would have
  to run, or on accounts a user cannot provision themselves.
- **Cross-platform by default.** Platform-specific code stays in that platform's package.
- **Code style is delegated to linters and formatters** once tooling is chosen — do not add
  formatting rules here.
- **Keep this file and the spec in sync.** When an open question in §6 is resolved, update both
  the spec and the status table above in the same change.
