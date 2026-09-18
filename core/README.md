# /core

Shared business logic for every Master Todo App client — crate `mtodo-core`.
Written once, depended on by all shells. No UI, no storage, no platform APIs.

## What exists now

Everything the Windows client runs on: the model, the parser, the recurrence
engine, and the local store.

```rust
let result = parse_at("gym every day 5pm", now);
// title "gym", time 17:00, recurrence Daily
```

- `task.rs` — `Task`, `Priority`, `Subtask`, `WeekdaySet`, the `Recurrence` rule
  enum, and per-occurrence `Exception`s
- `parse/` — the extraction pipeline: markers, then recurrence, then time, then
  date; whatever no extractor claimed becomes the title
- `recur.rs` — expands a rule into dates, applies exceptions, and advances a
  series on completion
- `store/` — SQLite persistence with a durable undo log
- `filter.rs` — the saved views and the narrowing (project, tag, search) the
  task list applies on top of the agenda
- `agenda.rs` — grouping everything ahead into Overdue / Today / Tomorrow /
  Next 7 days / Later / Someday, so every client agrees what "this week" means
- `daily.rs` — habits, streaks, the hand-entered durations, and the month
  journal

Ambiguity is resolved by guessing rather than asking, because capture speed is the
point of the app. Every guess lands in `ParseResult::guesses`, and
`ParseResult::matches` records the byte span each field came from — together they
let a client show what it inferred and let the user override any of it.

Dates and times are naive local values. The core carries no timezone; the shell
supplies one. Multi-device sync is what will force that question — see
`docs/todo-app-spec.md` §6.

Try it: `cargo run -p mtodo-core --example try_parse -- "gym every day 5pm"`

## What it recognizes

| | |
| --- | --- |
| Times | `5pm`, `5:30pm`, `17:00`, `at 5`, `noon`, `midnight` |
| | A time with no date means the next time that clock time comes round — today if it is still ahead, otherwise tomorrow. |
| Relative dates | `today`, `tomorrow`, `tonight`, `in 3 days`, `next week` |
| Weekdays | `friday`, `next friday`, `this friday`, `wed` |
| Absolute dates | `march 5`, `5th of march`, `oct 2nd 2027`, `2026-12-25` |
| Recurrence | `every day`, `daily`, `every other day`, `every 3 days`, `every monday`, `mondays`, `every monday and wednesday`, `every weekday`, `every 15th`, `every month on the 1st`, `every 6 months` |
| Markers | `#tag`, `@project`, `!p1` / `!high` / `!!!` |

A bare number is only a time when something marks it as one — an am/pm suffix, a
colon, or a preceding "at" — so "call 5 people" keeps its 5.

Any match can be *un-parsed*: `parse_excluding` takes byte ranges to leave as
plain text, which is what lets the capture field revert a phrase on backspace
instead of deleting a character.

## Not yet

- The sync client (waiting on `/sync-server`)
- Numeric slash dates (`12/25`), which need a locale decision first
- Relative times ("in 20 minutes"), and durations
- Full RFC 5545 recurrence. `Recurrence` covers the phrases the parser accepts;
  if the rules ever outgrow that, it becomes the input to an `rrule`-backed
  generator rather than growing more variants of its own.

## Commands

```sh
cargo test -p mtodo-core                  # 79 tests: corpus, recurrence, agenda, store
cargo clippy --workspace --all-targets
cargo fmt
```
