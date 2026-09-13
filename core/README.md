# /core

Shared business logic for every Master Todo App client — crate `mtodo-core`.
Written once, depended on by all shells. No UI, no storage, no platform APIs.

## What exists now

**Milestone 1: single-line natural-language parsing.**

```rust
let result = parse_at("gym every day 5pm", now);
// title "gym", time 17:00, recurrence Daily
```

- `task.rs` — the `Task` model and the `Recurrence` rule enum
- `parse/` — the extraction pipeline: recurrence, then time, then date; whatever
  no extractor claimed becomes the title

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
| Relative dates | `today`, `tomorrow`, `tonight`, `in 3 days`, `next week` |
| Weekdays | `friday`, `next friday`, `this friday`, `wed` |
| Absolute dates | `march 5`, `5th of march`, `oct 2nd 2027`, `2026-12-25` |
| Recurrence | `every day`, `daily`, `every other day`, `every 3 days`, `every monday`, `mondays`, `every weekday`, `every 15th`, `every month on the 1st`, `every 6 months` |

A bare number is only a time when something marks it as one — an am/pm suffix, a
colon, or a preceding "at" — so "call 5 people" keeps its 5.

## Not yet

Deliberately out of scope for milestone 1, in rough priority order:

- `!priority`, `#tag`, `@project` extraction from the same input line
- Multi-weekday rules — "every monday and wednesday" currently claims only the
  first weekday and reads the second as a due date
- Occurrence expansion and per-occurrence exceptions (this is where `rrule` comes
  in, with `Recurrence` as its input rather than growing more variants)
- Numeric slash dates (`12/25`), which need a locale decision first
- The SQLite store and the sync client

## Commands

```sh
cargo test -p mtodo-core        # unit + corpus + doc tests
cargo clippy --all-targets      # lint
cargo fmt                       # format
```
