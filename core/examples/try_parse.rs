//! Try the parser on a line: `cargo run -p mtodo-core --example try_parse -- "gym every day 5pm"`
//!
//! With no argument it runs a short demo set. Parsing resolves against the real
//! local time, so relative phrases mean what they would mean right now.

use mtodo_core::parse;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lines: Vec<String> = if args.is_empty() {
        ["gym every day 5pm", "call mom tomorrow at 5", "rent every 15th", "buy milk"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![args.join(" ")]
    };

    for line in lines {
        let result = parse(&line);
        let task = &result.task;
        println!("input:      {line:?}");
        println!("  title:      {:?}", task.title);
        println!("  date:       {:?}", task.date);
        println!("  time:       {:?}", task.time);
        println!("  recurrence: {:?}", task.recurrence);
        for m in &result.matches {
            println!("  matched {:?} from {:?}", m.field, m.text);
        }
        for g in &result.guesses {
            println!("  guessed {:?}: {}", g.field, g.note);
        }
        println!();
    }
}
