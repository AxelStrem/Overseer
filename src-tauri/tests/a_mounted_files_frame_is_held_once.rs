//! The frame a mounted node's formulas are worked out in is a copy of the whole mounted document,
//! and it is held once for the server, not once for every thread that worked a host out - see
//! `formula_evaluator`'s `FRAMES`. The server's pool keeps its threads between requests, and each
//! used to keep its own copies for as long as it lived, named by no figure of the memory line.

use overseer::heap::{self, Counting};
use overseer::server::DocumentRoot;
use overseer::formula_evaluator::FormulaEvaluator;
use std::sync::mpsc;
use std::time::Duration;

#[global_allocator]
static ALLOCATOR: Counting<std::alloc::System> = Counting(std::alloc::System);

const MB: usize = 1024 * 1024;

const HOST: &str = r#"tab weight (label="Weight") {
    mount CAL (lazy=false, mutable=false, hidden=true, source="side.os/s/History") { }
    float first = $(CAL/History.find("d1")/kcal)
}
"#;

fn side(days: usize) -> String {
    let mut text = String::from(
        r#"div (hidden=true) {
    div Day {
        string day = ""
        float grams = 0
        float kcal = $(grams * /s/per_gram)
        string note = "a note long enough to make every entry of the mounted list weigh something"
    }
}
tab s (label="S") {
    float per_gram = 2
    list History (entry=<Day>, key="day") {
"#,
    );
    for day in 1..=days {
        text.push_str(&format!("        - {{\n            - day = \"d{}\"\n            - grams = {}\n        }}\n", day, day));
    }
    text.push_str("    }\n}\n");
    text
}

fn held() -> usize {
    heap::flush_this_thread();
    heap::held()
}

#[test]
fn threads_that_worked_out_a_host_share_one_frame_of_its_mount_and_let_go_of_it_with_the_mount() {
    let root = std::env::temp_dir().join(format!("overseer_frame_once_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("host.os"), HOST).unwrap();
    std::fs::write(root.join("side.os"), side(3000)).unwrap();
    // Each thread works the host out itself, rather than the second being handed the first's.
    overseer::document_cache::set_budget_override(Some(0));
    overseer::document_disk::set_override(Some(None));
    let documents = DocumentRoot::new(&root).unwrap();
    let start = held();

    // Both threads stay up until the end, as the server's pool keeps its threads.
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let done_rx = std::sync::Arc::new(std::sync::Mutex::new(done_rx));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let documents = documents.clone();
        let done_rx = done_rx.clone();
        let (opened_tx, opened_rx) = mpsc::channel::<f64>();
        threads.push(std::thread::spawn(move || {
            let nodes = documents.open("host.os").expect("opens");
            let first = overseer::addressing::find(&nodes, "weight/first")
                .and_then(|n| n.parameters.get("_computed_value").cloned());
            drop(nodes);
            opened_tx
                .send(match first {
                    Some(overseer::types::OverseerValue::Float(f)) => f,
                    Some(overseer::types::OverseerValue::Integer(i)) => i as f64,
                    _ => f64::NAN,
                })
                .unwrap();
            heap::flush_this_thread();
            let _ = done_rx.lock().unwrap().recv();
        }));
        assert_eq!(opened_rx.recv().unwrap(), 2.0, "the mounted formula is worked out in its own file");
    }

    let (frames, bytes) = FormulaEvaluator::frames_held();
    assert_eq!(frames, 1, "one frame for the mount, whichever thread asked for it");
    assert!(bytes > MB, "the frame is a copy of the mounted list: {} bytes", bytes);

    overseer::document_cache::let_go_idle_everywhere(Duration::ZERO);
    assert_eq!(FormulaEvaluator::frames_held(), (0, 0), "the frame goes with the mount it was made from");
    // The snapshots of the mounted file's parse are kept on purpose, for a save to give back the
    // bytes it was given, and the memory line names them; what is left besides is the question.
    overseer::source_registry::SourceRegistry::forget_all_unused();
    let left = held().saturating_sub(start);
    assert!(
        left < bytes / 4,
        "with both threads still up, {} MB is held against a frame of {} MB",
        left / MB,
        bytes / MB
    );

    drop(done_tx);
    for thread in threads {
        thread.join().unwrap();
    }
    overseer::document_cache::set_budget_override(None);
    overseer::document_disk::set_override(None);
    let _ = std::fs::remove_dir_all(&root);
}
