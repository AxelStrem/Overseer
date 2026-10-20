//! A document held for its text is current for the minute it says, and no other.
//!
//! The cache is shared by every thread, and two of them can open the same text at different
//! minutes - two tests pinned to different instants, or two requests either side of a minute
//! turning over. A document was held in one step and its minute said in the next, so one thread's
//! document could end up marked with the other's minute, and was handed out as current when it
//! was a minute or an hour old.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::{OverseerNode, OverseerValue};

const TEXT: &str = "tab t (label=\"T\") {
    int out = $(millis_since_epoch(now()))
}
";

fn out(nodes: &[OverseerNode]) -> Option<i64> {
    for n in nodes {
        if n.name == "out" {
            return match n.parameters.get("_computed_value").or(n.parameters.get("value")) {
                Some(OverseerValue::Integer(i)) => Some(*i),
                Some(OverseerValue::Float(f)) => Some(*f as i64),
                _ => None,
            };
        }
        if let Some(found) = out(&n.children) {
            return Some(found);
        }
    }
    None
}

#[test]
fn every_thread_reads_its_own_minute() {
    let threads: Vec<_> = (0..8)
        .map(|t| {
            std::thread::spawn(move || {
                for round in 0..5000 {
                    let minutes = (t * 7 + round / 100 % 3) as i64 * 60;
                    let when = chrono::DateTime::from_timestamp(1_786_000_000 + minutes, 0).unwrap();
                    FormulaEvaluator::set_time_override(Some(when));
                    let nodes = app_api::load_document(TEXT.to_string()).expect("load");
                    FormulaEvaluator::set_time_override(None);
                    assert_eq!(
                        out(&nodes),
                        Some(when.timestamp_millis()),
                        "thread {t} at round {round} was handed a document worked out at another minute"
                    );
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("a thread failed");
    }
}
