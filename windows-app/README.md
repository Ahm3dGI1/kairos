# /windows-app

Phase 1 target: the Windows 11 client. A Tauri shell (chosen over Electron for footprint) wrapping `/core`.

Provides:
- System tray icon — hover to preview tasks, click for the quick panel
- Desktop-pinned widget — always visible, non-obscuring
- Global hotkey — Spotlight-style quick-add / quick-open overlay
- Calendar view, quick filters (Today / Overdue / Next 7 Days / by tag or project), search
- Keyboard-first navigation throughout, undo, reminders and notifications

All task semantics live in `/core`; this package is presentation and Windows platform integration only. No implementation yet.
