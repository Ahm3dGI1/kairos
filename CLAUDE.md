# Kairos

*καιρός* — the opportune moment, as against *chronos*, clock time. The name is the argument:
the useful question is not what time it is but which thing is worth doing now, and an app that
makes you fill in a form to record that has already lost.

## Why this project exists

Every capable todo app either paywalls the features that make it capable, locks your data in
someone else's cloud, or makes you fill out a form — date picker, time picker, recurrence
dialog — to record a thought you could have typed in four words. Kairos is the free,
open-source alternative: you type one line, "gym every day 5pm", and it becomes a fully
structured recurring task. It is offline-first by design, self-hostable end to end, and has no
paid tier. Phase 1 targets Windows 11; Linux, Android, and iPad follow later.

## What it does

A todo app whose defining interaction is single-line natural-language capture. Beyond capture, v1
covers priorities, tags, subtasks, projects/sections, recurring tasks with per-occurrence
exceptions, a calendar view, habits, a workout book, quick filters, search and
keyboard-first navigation — plus Windows-native touches: a tray icon with hover preview, an
always-visible desktop widget, and a global hotkey launcher. Everything is stored as plain Markdown you can edit in any editor; sync
is an optional layer on top, served by a server users host themselves.

## Tech stack

Phase 1 is built; the later platforms are still open. Do not treat a candidate as a commitment.

| Area          | Choice                                          | Status         |
| ------------- | ----------------------------------------------- | -------------- |
| Core logic    | Rust — `kairos-core`: parse, recur, store         | Built          |
| Windows shell | Tauri v2 — no bundler, static frontend           | Built          |
| Storage       | Markdown vault; SQLite as a rebuilt index        | Built          |
| Sync server   | Postgres + realtime, or a custom minimal server  | Candidate      |
| Linux client  | Rust TUI (ratatui) or the existing Go/Bubbletea  | **Open** (§6)  |
| Mobile client | React Native or Flutter                          | **Open** (§6)  |
| Auth (v1)     | None — shared API key per device                 | Decided        |

Rust won the core-language question: it is already Tauri's backend language, so the Windows shell
links it with no bridge, and it alone also serves a Linux TUI. Mobile pays an FFI layer (uniffi).
The Windows frontend is deliberately plain HTML/CSS/ES modules — no npm, no bundler, no
`node_modules` — so `cargo run` is the entire dev loop. Requires Rust 1.88+.

## Repo structure

Monorepo — one maintainer, one shared core, so a parsing fix lands everywhere in one PR. Split into
separate repos only if independent maintainers take over a platform.

```
/core          task model, NL parsing, recurrence, vault, store   (crate kairos-core)
/windows-app   Tauri shell: tray, widget, hotkey, calendar, UI    (crate kairos-windows)
```

A Linux client, mobile clients and a sync server are planned; they get directories when they get
code. A longer spec and the design boards the shell was built against live in `/docs`, which is
kept locally and not in version control.

## Build / test / lint

Both crates live in the root Cargo workspace.

```sh
cargo run -p kairos-windows      # launch the Windows app
cargo test -p kairos-core        # the fast loop: parser, recurrence, store
cargo clippy --workspace --all-targets
cargo fmt
cargo tauri build               # installers -> target/release/bundle
```

## Conventions

- **`/core` owns all task semantics.** Parsing, recurrence, and the task model live there and
  nowhere else. Clients are presentation and platform integration only — a Tauri command should be
  a thin wrapper over a core call, never a place where a rule gets restated.
- **The files are the system of record.** `/core/src/vault` owns a folder of Markdown files that
  holds every task, habit and workout; the SQLite database is an index rebuilt from it. Deleting
  the database must cost nothing. Any new kind of data needs a text form before it is done, and
  `Snapshot` is the only seam between the two — the store never learns about files, the vault
  never learns about SQL.
- **Offline-first is a hard requirement, not a fallback.** A feature that stops working without
  a network is a bug; sync reconciles later and is never in the critical path.
- **Self-hosting is the deployment model.** No dependency on a service the project would have
  to run, or on accounts a user cannot provision themselves.
- **Cross-platform by default.** Platform-specific code stays in that platform's package.
- **Code style is delegated to `rustfmt` and `clippy`** — do not add formatting rules here.
