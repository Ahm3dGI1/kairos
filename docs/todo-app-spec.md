# Master Todo App — Project Specification

## 1. Vision

A free, open-source, simplified-but-capable todo app. Core interaction: type or say one line ("gym every day 5pm") and it becomes a fully-structured task — no manual date/time/recurrence pickers required. Cross-platform, offline-first, no paywalled features.

**Phase 1 target platform: Windows 11.** Linux, Android, and iPad follow once the core and Windows shell are solid.

## 2. Repository Structure

**Decision: monorepo.**

```
/core           — shared business logic (task model, NL parsing, sync client)
/windows-app    — Tauri shell: tray icon, widget, hotkey launcher, UI
/linux-app      — terminal client (future; may build on existing Go/Bubbletea TUI)
/mobile-app     — React Native or Flutter client (future)
/sync-server    — lightweight self-hostable sync backend
/docs
```

Rationale: one maintainer, one shared core depended on by every client — a monorepo keeps cross-cutting changes (e.g. a parsing bug fix) landing everywhere in one PR. Split into separate repos later only if independent maintainers take over individual platforms.

## 3. Architecture

- **Core logic**: written once (candidate: Rust, given existing Rust experience from Gofi; reusable natively on Windows/Linux and via FFI on mobile later). Owns the task model, natural-language parsing, and sync client.
- **Windows 11 shell**: Tauri (lighter than Electron). Provides:
  - System tray icon — hover to preview tasks, click to open quick panel
  - Desktop-pinned "widget" view — always visible, non-obscuring
  - Global hotkey — Spotlight-style quick-add/quick-open overlay
- **Linux shell** *(future)*: terminal UI — either extends the existing Go/Bubbletea todo project or a new Rust TUI (ratatui) sharing the Rust core.
- **Mobile shell** *(future)*: React Native or Flutter.
- **Sync & hosting**: local-first. Each device holds a local SQLite store; sync is an optional layer via a lightweight, self-hostable sync server (e.g. Supabase-style Postgres + realtime, or a custom minimal server). Users self-host their own instance — no centrally-hosted service, no accounts to manage on your end.
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
- Calendar view of tasks
- Quick filters (e.g. Today, Overdue, Next 7 Days, by tag/project)
- Search across all tasks
- Keyboard-first navigation throughout

### Windows-specific UX
- System tray icon with hover preview / click-to-open
- Always-visible desktop widget (non-intrusive, doesn't obscure active work)
- Global hotkey launcher for instant quick-add/quick-open

### Reliability & trust
- Offline-first (core requirement, not a fallback)
- Undo for deletions/completions
- Reminders/notifications

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
