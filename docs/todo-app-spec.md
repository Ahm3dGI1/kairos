# Kairos — Project Specification

## 1. Vision

**The name.** *Kairos* (καιρός) is the Greek for the opportune moment — the right time to act —
as distinct from *chronos*, time as it elapses on a clock. Every other todo app is built around
chronos: fields for dates, fields for times, a calendar to file things into. This one is built
around the other question, which is why the defining interaction is a sentence rather than a form.

A free, open-source, simplified-but-capable todo app. Core interaction: type or say one line ("gym every day 5pm") and it becomes a fully-structured task — no manual date/time/recurrence pickers required. Cross-platform, offline-first, no paywalled features.

**Phase 1 target platform: Windows 11.** Linux, Android, and iPad follow once the core and Windows shell are solid.

## 2. Repository Structure

**Decision: monorepo.**

```
/core           — shared business logic: task model, NL parsing, recurrence, vault, store
/windows-app    — Tauri shell: tray icon, widget, hotkey launcher, UI
/docs           — this spec, and the design boards the shell is built against
```

`/linux-app`, `/mobile-app` and `/sync-server` are planned and have no code. They are described
in §3 and §6; they get directories when they get contents, rather than standing empty.

Rationale: one maintainer, one shared core depended on by every client — a monorepo keeps cross-cutting changes (e.g. a parsing bug fix) landing everywhere in one PR. Split into separate repos later only if independent maintainers take over individual platforms.

## 3. Architecture

- **Core logic**: written once (candidate: Rust, given existing Rust experience from Gofi; reusable natively on Windows/Linux and via FFI on mobile later). Owns the task model, natural-language parsing, and sync client.
- **Windows 11 shell**: Tauri (lighter than Electron). Provides:
  - System tray icon — hover to preview tasks, click to open quick panel
  - Desktop-pinned "widget" view — always visible, non-obscuring
  - Global hotkey — Spotlight-style quick-add/quick-open overlay
- **Linux shell** *(future)*: terminal UI — either extends the existing Go/Bubbletea todo project or a new Rust TUI (ratatui) sharing the Rust core.
- **Mobile shell** *(future)*: React Native or Flutter.
- **Storage** *(decided, built)*: a **vault of plain text files is the system of record**, and the local SQLite store is an index rebuilt from it. Deleting the database costs nothing; deleting the vault is what loses data. The reasoning is the vision's: an app whose data you cannot open in a text editor is one you do not own, and "no lock-in" has to mean something stronger than an export button. The formats are the ones a person would have written anyway — a task is a Markdown checklist item, a habit month is a list of days, a workout is `- Bench press: 10x60, 8x65` — so the same folder is legible to the user, to Obsidian, and to an AI agent pointed at it.
  - Metadata on a task line is read as a **suffix**, so ordinary prose in a title is never eaten.
  - A line with **no `^id` is new** and goes through the natural-language parser, so typing `- [ ] gym every day 5pm` into a file is the same act as typing it into the capture bar. Lines that carry an id are read literally.
  - Writing is content-addressed — a file is touched only when what it should hold differs from what it does — which is what stops the file watcher chasing the app's own writes.
  - `Snapshot` is the only seam: the store never learns about files, the vault never learns about SQL.
- **Settings** *(decided, built)*: one switch per feature, defined in the core so every shell offers the same list, stored as `settings.json` inside the vault. A preference you cannot read without launching the app is one you cannot fix when the app will not launch.
- **Sync & hosting**: local-first. Each device holds a local vault and its SQLite index; sync is an optional layer via a lightweight, self-hostable sync server (e.g. Supabase-style Postgres + realtime, or a custom minimal server). Users self-host their own instance — no centrally-hosted service, no accounts to manage on your end. A text vault also makes the crude version of sync — put the folder in Dropbox or a git repo — work without any server at all.
- **Auth (v1)**: none. Device-to-own-sync-instance uses a shared API key/secret. Full OAuth (Google sign-in) is a stretch goal only if this becomes a hosted multi-tenant product later.
- **Offline-first**: full functionality with no network connection; sync reconciles when back online.

## 4. Core Features (v1 — Windows 11)

### Task capture
- Single-line quick-add, optimized for speed
- Natural-language parsing of the input line to auto-extract:
  - Date
  - Time
  - Recurrence (e.g. "every day," "every Monday")
- Manual override of any parsed field after entry

### Task properties
- Priority levels
- Tags / labels (independent of priority; multiple per task; filterable)
- Subtasks / checklists within a task
- Sections / projects (grouping beyond a flat list)
- Recurring task exceptions — reschedule or skip a single occurrence without breaking the series

### Views & navigation
- **One task list, grouped rather than tabbed** — Overdue, Today, Tomorrow, Next 7 days, Later, Someday in a single scroll. Separate Today/Overdue/Next-7 tabs made the user do the sorting; the buckets are headings, not destinations. Completed work is a toggle.
- Narrowing by project or tag, and search across all tasks
- Calendar view of tasks, with recurrence expanded
- Keyboard-first navigation throughout

### Capture field behaviour
- Each phrase the parser claims is highlighted in place in the input
- **Backspace against a highlight un-parses it**, returning the words to the title — the same gesture that undoes an autoformat in a document. The parser supports this directly (`parse_excluding`), so no client has to re-implement it.

### Habits and journal
Separate from tasks, because a habit is not a task: a task is done once and gone, while a habit is a question about consistency, and completing one must not remove it.

- A **month grid**: habits as rows, days as columns, one tick per cell. Consistency is not visible one day at a time.
- **Screen time and sleep** as numeric rows in the same grid, entered by hand — reading them automatically would mean a background agent or a vendor API, which the offline-first, self-hosted model (§3) rules out.
- A **journal for the month**, not the day. Entries carry their own headings, so a day, a week or a trip can each be one block.

### Windows-specific UX
- System tray icon with hover preview / click-to-open
- **Sticky note**: a small panel of the day's tasks that behaves like an ordinary window — it drops behind whatever you focus next — with a pin for when it should stay above everything. Always-on-top is intrusive by nature and is never the default.
- Global hotkey launcher for instant quick-add/quick-open, sharing the capture field with the main window

### Reliability & trust
- Offline-first (core requirement, not a fallback)
- Undo for deletions/completions
- Reminders/notifications
- Data in plain text the user owns, readable and editable without the app
- A JSON backup of the database before any import replaces it, last five kept

## 5. Stretch Goals (post-v1)

- Local speech-to-text dictation (on-device APIs first; self-hosted Whisper as fallback)
- AI integration (e.g. smarter parsing, task suggestions)
- Gmail / Google Calendar integration (view calendar + surface relevant emails)
- Linux terminal client
- Mobile clients (Android, iPad)
- Optional hosted sync + accounts (only if the project scales beyond self-hosting)

## 6. Open Questions

- ~~Final choice of core language (Rust vs. TypeScript)~~ — **resolved: Rust.** Tauri's backend is already Rust, so the Windows shell links `/core` natively; a Linux TUI can share it too. Mobile pays for this with an FFI layer (uniffi) when it arrives.
- Sync conflict-resolution strategy (needed once multi-device sync is built)
- Whether the Linux client reuses/extends the existing Go/Bubbletea TUI project or is rebuilt in Rust for core-sharing
