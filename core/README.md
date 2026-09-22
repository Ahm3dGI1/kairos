# /core

Shared business logic for every Kairos client — crate `kairos-core`.
Written once, depended on by all shells. No UI, no platform APIs.

```rust
let result = parse_at("gym every day 5pm", now);
// title "gym", time 17:00, recurrence Daily
```

Try it: `cargo run -p kairos-core --example try_parse -- "gym every day 5pm"`

## What is in here

| | |
| --- | --- |
| `task.rs` | `Task`, `Priority`, `Subtask`, `WeekdaySet`, the `Recurrence` rule enum, per-occurrence `Exception`s |
| `parse/` | the extraction pipeline: markers, then recurrence, then time, then date; whatever no extractor claimed becomes the title |
| `recur.rs` | expands a rule into dates, applies exceptions, advances a series on completion |
| `vault/` | the Markdown files that hold everything — the system of record |
| `store/` | the SQLite index built from them, with a durable undo log |
| `agenda.rs` | grouping into Overdue / Today / Tomorrow / Next 7 days / Later / Someday, so every client agrees what "this week" means |
| `filter.rs` | the saved views, and the narrowing a list applies on top of them |
| `daily.rs` | habits, their kinds, streaks, and the month journal |
| `workout.rs` | routines, exercises, sessions and sets |
| `prayer.rs` | prayer times, from the date and a pair of coordinates |
| `settings.rs` | every switch a client should offer, described so it can render them |

## The two things worth knowing

**The files are the record.** `vault/` owns a folder of Markdown that holds
every task, habit and workout; the database is an index rebuilt from it, and
deleting it costs nothing. `Snapshot` is the only seam between the two — the
store never learns about files, the vault never learns about SQL. Anything new
needs a text form before it is done.

**Ambiguity is guessed, not asked about**, because capture speed is the point.
Every guess lands in `ParseResult::guesses`, and `ParseResult::matches` records
the byte span each field came from — together they let a client show what it
inferred and let the user override any of it. Any match can be *un-parsed*:
`parse_excluding` takes byte ranges to leave as plain text, which is what lets
the capture field revert a phrase on backspace instead of deleting a character.

Dates and times are naive local values. The core carries no timezone; the shell
supplies one. Multi-device sync is what will force that question.

## What the parser recognizes

| | |
| --- | --- |
| Times | `5pm`, `5:30pm`, `17:00`, `at 5`, `noon`, `midnight` |
| | A time with no date means the next time that clock time comes round — today if it is still ahead, otherwise tomorrow. |
| Relative dates | `today`, `tomorrow`, `tonight`, `in 3 days`, `next week` |
| Weekdays | `friday`, `next friday`, `this friday`, `wed` |
| Absolute dates | `march 5`, `5th of march`, `oct 2nd 2027`, `2026-12-25` |
| Recurrence | `every day`, `daily`, `every other day`, `every 3 days`, `every monday`, `mondays`, `every monday and wednesday`, `every mon/wed/fri`, `every weekday`, `every 15th`, `every month on the 1st`, `every 6 months` |
| Markers | `#tag`, `@project`, `!p1` / `!high` / `!!!` |

A bare number is only a time when something marks it as one — an am/pm suffix,
a colon, or a preceding "at" — so "call 5 people" keeps its 5.

## Not yet

- A sync client. A vault in a synced folder covers most of it meanwhile.
- Numeric slash dates (`12/25`), which need a locale decision first.
- Relative times ("in 20 minutes"), and durations.
- Full RFC 5545 recurrence. `Recurrence` covers the phrases the parser accepts;
  if the rules ever outgrow that, it becomes the input to an `rrule`-backed
  generator rather than growing more variants of its own.

## Commands

```sh
cargo test -p kairos-core
cargo clippy --workspace --all-targets
cargo fmt
```
