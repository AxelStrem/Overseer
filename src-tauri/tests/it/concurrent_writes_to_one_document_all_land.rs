//! Writes to one document at the same time are applied one after another, and none is lost.
//!
//! Each addressed write reads the file, works the change out and writes the whole text back. With
//! nothing holding other writers off in between, two writes at once both started from the same
//! text and the second to finish wrote over the first - and both renamed the same temporary file
//! over the document, so one of them could fail outright. Two agent loops moving items of one
//! tracker found it: a move answered success and was gone when the item was read back.

use crate::common::shared;
use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;

const ENTRIES: usize = 8;

fn document() -> String {
    let mut text = String::from(
        "tab board (mutable=true) {\n    div (hidden=true) {\n        div Item {\n            string name = \"\"\n            string stage = \"ready\"\n        }\n    }\n\n    list Items (entry=<Item>, key=\"name\") {\n",
    );
    for n in 0..ENTRIES {
        text.push_str(&format!("        - {{\n            - name = \"h{}\"\n        }}\n", n));
    }
    text.push_str("    }\n}\n");
    text
}

#[test]
fn concurrent_sets_to_different_entries_of_one_document_all_land() {
    let _shared = shared();
    let root = std::env::temp_dir().join(format!("overseer_writelock_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    let service = DocumentRoot::new(&root).expect("a root");

    for round in 0..10 {
        std::fs::write(root.join("b.os"), document()).expect("write");
        let stage = format!("testing{}", round);
        let start = std::sync::Barrier::new(ENTRIES);
        let answers: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..ENTRIES)
                .map(|n| {
                    let (service, start, stage) = (&service, &start, &stage);
                    scope.spawn(move || {
                        start.wait();
                        service
                            .set_at(
                                "b.os",
                                &format!("board/Items/[h{}]/stage", n),
                                OverseerValue::String(stage.clone()),
                            )
                            .map(|_| ())
                            .map_err(|e| format!("{:?}", e))
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("a writer panicked")).collect()
        });
        for (n, answer) in answers.iter().enumerate() {
            assert!(answer.is_ok(), "round {}: the write to h{} was refused: {:?}", round, n, answer);
        }
        let text = std::fs::read_to_string(root.join("b.os")).expect("read");
        let landed = text.matches(&format!("stage = \"{}\"", stage)).count();
        assert_eq!(
            landed, ENTRIES,
            "round {}: {} of {} writes answered success and were lost:\n{}",
            round,
            ENTRIES - landed,
            ENTRIES,
            text
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
