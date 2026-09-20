---
noteId: "9863a3e0b39211f1ae534ffda4b4a81d"
tags: []

---

# Kairos — design brief

Everything the app currently is, in enough detail to redesign it. Written for a
designer who has not seen the code.

Nothing in here is sacred except the constraints in §2 and the principles in
§1.3. The current visual language (§12) is a competent first pass, not a
direction to preserve — treat it as the thing being replaced.

---

## 1. The product

### 1.1 What it is

A free, open-source, offline-first todo app for Windows 11. The defining
interaction is **single-line capture**: you type one line —
`gym every day 5pm @health #fitness !p1` — and it becomes a fully structured
recurring task. No date picker, no time picker, no recurrence dialog.

It also carries a **habit tracker** and a **journal**, because the same person
who wants to know what is due today also wants to know whether they have been
going to the gym.

### 1.2 Who it is for

One person, on their own machine, with their own data. There is no account, no
server, no paid tier. Sync (later) is to a server the user hosts themselves.

### 1.3 Principles that should survive any redesign

1. **Capture speed is the product.** Anything that adds a step between having a
   thought and it being recorded is a regression. The app never asks a
   follow-up question during capture.
2. **Guess, then show the guess.** Ambiguity is resolved by deciding, not by
   prompting — but every inference is visible and reversible.
3. **Offline-first is a hard requirement.** Nothing may depend on a network.
4. **Keyboard-first.** Every action has a key. The mouse is optional everywhere.
5. **Quiet by default.** This is a tool that sits open all day next to real
   work. It should never shout, animate for its own sake, or steal focus.
6. **Data outlives the UI.** Completion, deletion and edits are undoable.

---

## 2. Hard constraints

| Constraint | Detail |
| --- | --- |
| Platform | Windows 11 desktop, Tauri v2 (a native window wrapping a WebView2 webview) |
| Rendering | Chromium (WebView2). Modern CSS is available: grid, `color-mix()`, container-ish layouts, `:has()` |
| Frontend stack | **Plain HTML, CSS and ES modules. No framework, no bundler, no npm.** A design must be buildable without React/Tailwind/etc. |
| Fonts | System only — no webfonts. Segoe UI Variable / Segoe UI are the safe Windows choices; a monospace fallback for keys and numbers |
| Icons | Currently bare Unicode glyphs (✓ ✕ ⚑ 🗓 ↻ 📌). Inline SVG is fine and probably better. No icon-font dependency |
| Theme | **Light and dark are both first-class**, driven by `prefers-color-scheme`. Neither may be an afterthought |
| Display | Typically 150% Windows scaling. Design at CSS-pixel sizes; assume ~1024×576 CSS px of usable screen on a 1536×864 laptop |
| Main window | 900×560 CSS px default, **min 460×360**, resizable. Must stay usable at the minimum |
| Motion | Respect `prefers-reduced-motion`. Transitions should be short (≤150ms) or absent |

---

## 3. Surfaces

The app is four windows and a tray icon.

| Surface | Size | Chrome | Purpose |
| --- | --- | --- | --- |
| **Main window** | 900×560, resizable | Standard title bar | Three pages: Tasks, Calendar, Habits |
| **Quick-add overlay** | 680×132, fixed | **Frameless**, always-on-top, centred, not in taskbar | Summoned anywhere by `Ctrl+Shift+Space`. One field. Enter adds and dismisses |
| **Sticky note** | 320×420, resizable | **Frameless**, in taskbar | Today's tasks. An ordinary window that drops behind what you focus; a pin makes it stay on top |
| **Tray icon** | — | — | Hover previews the day; click opens; right-click menu |

Closing any window hides it rather than quitting — the app lives in the tray.
Quit is only on the tray menu.

---

## 4. Main window — shared shell

Top to bottom:

1. **Capture bar** (§5) — visible on Tasks and Calendar, **hidden on Habits**
2. **Page bar** — segmented control `Tasks | Calendar | Habits`, plus (Tasks
   only, right-aligned) a search field and a "Completed" checkbox
3. **Body** — an optional left sidebar, the page content, an optional right
   detail pane
4. **Footer** — keyboard hints on the left, a one-line day summary on the right
   ("3 due today, 1 overdue" / "Nothing due today")

Floating elements: a **toast** (bottom centre, ~3.2s) for confirmations and undo
prompts, and an **error bar** (top, full width, red) when a command fails.

**Sidebar** (Tasks page only): a single "Everything" row, then a *Projects*
list, then a *Tags* list. Selecting one narrows the task list; clicking the
active one clears it. Projects/tags come from what is in use — there is no
management UI for them.

---

## 5. The capture bar — the most important control

A single-line text field plus an "Add" button, with a chip row beneath it.

### 5.1 Token pills (in the field)

As you type, every phrase the parser claims gets a **tinted pill drawn behind
the text, inside the input**. Currently: blue for date/time/recurrence, accent
for `#tag` and `@project`, amber for `!priority`.

> Implementation note for whoever builds it: this is a mirror `<div>` behind a
> transparent `<input>`, both sharing identical font metrics and padding. A
> design that changes the field's typography must keep the two in lockstep.

### 5.2 Backspace reverts a parse

With the caret immediately after a pill, **Backspace un-parses that phrase**
instead of deleting a character: the pill disappears and the words fall back
into the task title. This is the "undo an autoformat" gesture from a document
editor. A small "N reverted" chip appears in the preview row while any are
reverted.

This is a signature interaction and needs to read clearly — the moment the pill
vanishes is the feedback.

### 5.3 Preview chips (below the field)

A row of chips showing the *result*: the title (emphasised), the resolved date,
the time, the recurrence (`↻ every other friday`), priority, project, tags.
Plus **guess chips**, styled differently (currently dashed, amber), for
inferences the parser made — e.g. `"monday" resolved to 2026-09-21`, or
`07:00 has passed today; due tomorrow`.

### 5.4 States to design

Empty (placeholder), typing with no matches, typing with several pills, one or
more reverted, and a long line that scrolls horizontally (pills must scroll with
the text).

---

## 6. Tasks page

### 6.1 One list, grouped — not tabs

This replaced a row of Today / Next 7 Days / All / Overdue / Inbox / Completed
tabs. The tabs made the user do the sorting. Now everything ahead is one scroll
with section headings, in this fixed order:

| Section | Contains |
| --- | --- |
| **Overdue** | Past its date, still open. Visually the most urgent thing on the page |
| **Today** | Due today, or a recurring task whose next occurrence is today |
| **Tomorrow** | — |
| **Next 7 days** | Days 2–7 |
| **Later** | Dated, further out |
| **Someday** | No date at all |
| **Completed** | Only when the "Completed" toggle is on |

Empty sections are not rendered. Each heading carries a count. Inside a section:
soonest first, then by priority.

### 6.2 Task row anatomy

- A **circular checkbox** on the left. Clicking completes; a completed row shows
  a filled check and a struck-through title
- **Title**
- A **meta line** underneath, any of: due date, time (`4:00 PM`), recurrence
  (`↻ every weekday`), project (`@work`), tags (`#health`), subtask progress
  (`2/5`)
- A **priority stripe** down the left edge — red / amber / accent for
  high / medium / low, nothing for unprioritised
- A **selected state** for the keyboard cursor, distinct from hover

Inside the Today and Tomorrow sections the date is omitted from the meta line —
the heading already says it.

### 6.3 Detail pane (right side, ~300px)

Deliberately minimal. Top to bottom:

1. **A chip row**: a date chip (`🗓 Tomorrow` / `No date`), a priority flag chip
   (`⚑ High` / `Priority`), and a close ✕ pushed right
2. **The title**, editable in place, auto-growing, no visible input chrome
3. To the right of the title, an **icon that toggles the body** between two
   modes, plus subtask progress when there are subtasks
4. **The body** — either a free-text notes area (markdown-ish, unrendered) *or*
   a subtask checklist with an "Add a subtask…" field. One or the other, never
   both
5. **A footer** of buttons: Complete/Reopen, then (recurring tasks only) "Skip
   one" and "Push a day", then Delete

**The date chip opens a popover** containing, in order: a mini month calendar
(Monday-first, today outlined, selection filled), quick buttons
`Today / Tomorrow / Next week`, a **Time** field, a **Repeat** select, and a
"Clear date" button. A recurrence the picker cannot express is shown as a note
rather than silently reset.

**The priority chip opens a small menu**: High / Medium / Low / None, each in
its own colour.

Everything else about a task is folded away behind those two chips. That is the
point — the pane should show what a task *is*, and only unfold a picker when
asked.

### 6.4 Empty states

- No tasks at all: *"Nothing ahead. Press N to add something."*
- Search with no hits: *"Nothing matches that search."*

---

## 7. Calendar page

A month grid, Monday-first, 6 rows × 7 columns. Header: `‹ September 2026 ›` and
a "Today" button. Each cell shows the day number and the tasks occurring that
day — **with recurrence expanded**, so a daily task appears on all 30 days.
Today's cell is outlined; days outside the month are dimmed. Clicking a task
jumps back to the Tasks page with it selected.

Cells grow to fit the busiest day. The current design has no "+2 more"
affordance and probably should.

---

## 8. Habits page

No capture bar here. Header: `‹ September 2026 ›` and a "This month" button.
Two columns: the grid on the left, the journal on the right.

### 8.1 The month grid

A spreadsheet-like grid, horizontally scrollable:

- **Rows are habits**, columns are the days of the month
- Column headers show the weekday initial above the day number; today is
  highlighted; weekends are tinted
- **The habit-name column is sticky** while the days scroll
- Each cell is a tick target. Done = filled accent. **Future days are dimmed and
  not clickable** — a day that has not happened cannot have been done
- A **total column** on the right: days done this month, plus a current-streak
  badge (`12d`)
- Habit names rename on double-click; a delete ✕ appears on row hover
- Below the habits, two **numeric rows** in the same grid — *Screen time* and
  *Sleep* — entered by hand. Cells show hours (`7.5`); clicking one turns it
  into a text input that accepts `7h30`, `7.5h`, `450` or `90m`. Their total
  column shows the month average
- An "Add a habit…" field sits under the grid

A **streak does not break on today being untouched** — the day is not over. The
count simply starts from yesterday until you tick it.

### 8.2 The month journal (right column)

Not one box per day. A month-long page made of **entries**, each with:

- A **heading** — usually a day (`Friday 18`), but it can be anything
  (`The trip`)
- A **body** that grows as you type
- A delete ✕ on hover

A "+ Entry" button adds one, pre-headed with today's date when today is in the
month being viewed. Empty entries are discarded. Saving is automatic and
debounced.

Empty state: *"Nothing written this month. Add an entry to start."*

---

## 9. Quick-add overlay

Frameless, centred, always on top, summoned by `Ctrl+Shift+Space` from anywhere
in Windows. It is the capture bar at a larger size (17px), with the same pills
and the same backspace-revert, plus the preview chip row.

- `Enter` adds and dismisses
- `Shift+Enter` adds and **stays open** for a run of captures
- `Escape`, or losing focus, dismisses

It must look like a spotlight — a floating card with a shadow, not a window.

---

## 10. Sticky note

Frameless 320×420 panel listing today's tasks.

- A drag-region header: `Today`, a right-aligned count (`1 due today`), then
  three buttons — **pin**, open-main (`↗`), close (`✕`)
- Rows: a complete button, the title, and a right-aligned time or `Overdue`
- **Unpinned (default)**: an ordinary window that drops behind whatever you
  focus next. **Pinned**: stays above everything. The pin's state must be
  obvious at a glance — currently greyed vs. lit
- It never takes focus when it appears
- Empty state: *"Nothing due today."*

---

## 11. Tray and notifications

- **Tray tooltip** previews the day: a headline (`3 due today, 1 overdue`) then
  up to three task titles and `+N more`
- **Right-click menu**: Open Kairos · Quick add (Ctrl+Shift+Space) ·
  Toggle sticky note · — · Quit
- **Left click** opens the main window
- **Reminders** fire a standard Windows notification at a task's time —
  title = the task, body = `Due at 5:00 PM`

---

## 12. Current visual language (the thing being replaced)

Fluent-adjacent: Segoe UI Variable, 8px radii, quiet surfaces, one accent
colour. Offered so a redesign knows what it is departing from.

```
                 light      dark
accent           #5850ec    #8b85ff
accent-soft      #eceafe    #26244a
bg               #f6f6f8    #17171b
surface          #ffffff    #202026
surface-2        #f1f1f4    #2a2a32
border           #e2e2e7    #33333d
text             #16161a    #f2f2f5
text-dim         #63636e    #a8a8b4
text-faint       #8e8e99    #7c7c88
danger           #c4314b    #ff7a90
warn             #b45309    #f0b429
ok               #107c41    #4ade80
radius 8px · body 14px · meta 12px · headings 11px uppercase tracked
```

Known weaknesses worth fixing: the density is uniform (everything is 12–14px, so
nothing leads); group headings are quiet to the point of invisibility; the
sidebar and the detail pane compete for the same attention; Unicode glyphs sit
inconsistently on the baseline; and there is no visual distinction between a
task that is merely dated and one that is genuinely urgent beyond a 3px stripe.

---

## 13. Data the UI has to show

Useful when deciding what a row, chip or cell can display.

**Task** — id · title · notes · due date · time · recurrence · per-occurrence
exceptions · priority (High/Medium/Low/None) · tags (many) · project (one or
none) · subtasks (title + done) · completed date · created date.

**Recurrence** — Daily · Every N days · Weekly on a set of weekdays · Every N
weeks on a set of weekdays · Monthly (optionally on a given day) · Every N
months · Yearly. Always renderable as a phrase: `every other friday`,
`every month on the 1st`, `every weekday`.

**Habit** — id · name · created · archived · position. **Day log** — date ·
screen minutes · sleep minutes · which habits were ticked.
**Month journal** — year/month · ordered entries of (heading, body).

---

## 14. What the parser understands

This is the vocabulary that can appear as a pill, so it bounds what the capture
bar ever has to display.

| Kind | Examples |
| --- | --- |
| Times | `5pm` · `5:30pm` · `17:00` · `at 5` · `noon` · `midnight` |
| Relative dates | `today` · `tomorrow` · `tonight` · `in 3 days` · `next week` |
| Weekdays | `friday` · `next friday` · `this friday` · `wed` |
| Absolute dates | `march 5` · `5th of march` · `oct 2nd 2027` · `2026-12-25` |
| Recurrence | `every day` · `daily` · `every other day` · `every 3 days` · `every monday` · `mondays` · `every monday and wednesday` · `every weekday` · `every 15th` · `every month on the 1st` · `every 6 months` |
| Markers | `#tag` · `@project` · `!p1` / `!high` / `!!!` |

Two behaviours worth knowing: a bare time with no date means *the next time that
clock time comes round* (today if still ahead, otherwise tomorrow, flagged as a
guess); and a bare number is only a time when something marks it as one, so
`call 5 people` keeps its 5.

---

## 15. Keyboard map

| Key | Action |
| --- | --- |
| `N` | Focus capture |
| `/` | Focus search |
| `J` / `K` or arrows | Move the selection |
| `Space` / `X` | Complete |
| `Enter` | Open the detail pane |
| `E` | Edit the selected task |
| `Del` | Delete |
| `U` or `Ctrl+Z` | Undo |
| `1` `2` `3` | Tasks · Calendar · Habits |
| `W` | Toggle the sticky note |
| `Esc` | Close the pane / clear the search / dismiss the overlay |
| `Ctrl+Shift+Space` | Quick-add overlay, from anywhere in Windows |

A design must leave room for a **visible selection state** that is distinct from
hover, and ideally show shortcut affordances without clutter (the current footer
hint strip is one answer, probably not the best one).

---

## 16. States and edge cases to design for

- Zero tasks, zero habits, zero journal entries, first run
- A section with 40 tasks; a calendar day with 8 tasks; a habit list with 15
  habits over a 31-day month
- Very long task titles, long project and tag names
- A completed recurring task (advances to its next occurrence rather than
  disappearing) and the toast that explains it
- An overdue recurring task
- Undo affordance after completing or deleting (currently a toast saying
  "U to undo")
- Error bar when a write fails
- The window at its 460×360 minimum — the sidebar and detail pane currently just
  disappear below 720px, which is a cop-out
- Light and dark for every one of the above

---

## 17. Not built yet (do not design around these as if they exist)

- Sync between devices, and any account or sharing UI
- Choosing what the sticky note shows (it is fixed to Today)
- Editing a recurrence beyond the seven presets in the picker
- Drag-and-drop reordering, manual sort orders
- Sub-projects or nested sections
- Rendered markdown in notes (the field is plain text)
- Per-task reminder offsets ("15 minutes before")
- Speech-to-text capture, AI assistance, calendar/email integration

---

## 18. What would help most from a redesign

In rough priority:

1. **A visual hierarchy that makes "what should I do now" obvious** in one look
   — the grouped list is right structurally but flat visually.
2. **A treatment for the token pills and preview chips** that makes the parse
   feel magical rather than technical, and makes the backspace-revert readable.
3. **A detail pane that feels calm** while still holding a calendar, a repeat
   rule, priority, notes and subtasks behind two chips.
4. **A habit grid that is legible at 31 columns** without becoming a spreadsheet.
5. **A coherent icon set** to replace the Unicode glyphs.
6. **A density and type scale** with more than one voice, in light and dark.

Several distinct directions are more useful than one refined one.
