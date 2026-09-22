use kairos_core::parse;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lines: Vec<String> = if args.is_empty() {
        [
            "gym every day 5pm",
            "call mom tomorrow at 5",
            "submit report every monday 9am @work #urgent !p1",
            "buy milk",
        ]
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
        println!("  due:        {:?}", task.due);
        println!("  time:       {:?}", task.time);
        println!("  recurrence: {:?}", task.recurrence);
        println!("  priority:   {:?}", task.priority);
        println!("  project:    {:?}", task.project);
        println!("  tags:       {:?}", task.tags);
        for m in &result.matches {
            println!("  matched {:?} from {:?}", m.field, m.text);
        }
        for g in &result.guesses {
            println!("  guessed {:?}: {}", g.field, g.note);
        }
        println!();
    }
}
