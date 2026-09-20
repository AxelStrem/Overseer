//! An action on an entry made from a template reaches the file.
//!
//! A button on a list entry writes a field the entry does not mention - the template supplies it
//! - so the write has to be recorded as an override, or the serializer treats the value as
//! inherited and leaves it out. `set` and `toggle` recorded it; `inc` did not, and the difference
//! went unnoticed because `inc` on a field outside a list works perfectly.
//!
//! Inside a list it did not work at all. Pressing answered Ok, the page repainted with the new
//! number, no step was left to take back, and the file never moved - so a refresh put the number
//! back. That is the shape a write should never have: loud success, silent nothing.
//!
//! Reported from ordinary use as "I clicked did it on one task, then another, and was told the
//! document had changed since the page opened it". That was this and the page's stale baseline
//! together, which is why the two are checked in one place.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab tasks (label="Tasks", mutable=true) {

    div (hidden=true) {

        div Task (layout="horizontal") {
            string name = ""
            int done = 0
            bool flag = false
            timestamp showing (mutable="guarded") = $(today())
            button bump (label="did it") {
                on click {
                    inc (path="../done", by=1)
                }
            }
            button mark (label="set") {
                on click {
                    set (path="../done") = 7
                }
            }
            button flip (label="toggle") {
                on click {
                    toggle (path="../flag")
                }
            }
            button back (label="back") {
                on click {
                    set (path="../showing") = $(date_add_days(../showing, -1))
                }
            }
        }
    }

    list Open (entry=<Task>, key="name") {
        - {
            - name = "alpha"
        }
        - {
            - name = "beta"
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_action_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("t.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("t.os")).expect("read")
}

/// The path of names the page sends for the node at an address.
fn names(service: &DocumentRoot, address: &str) -> Vec<String> {
    let nodes = service.open("t.os").expect("open");
    overseer::addressing::name_path(&nodes, address)
        .unwrap_or_else(|| panic!("no such address: {}", address))
}

fn press(service: &DocumentRoot, entry: &str, button: &str) -> serde_json::Value {
    let path = names(service, &format!("tasks/Open/[{}]/{}", entry, button));
    service
        .command_for(
            "alice",
            Some("t.os"),
            "run_overseer_event",
            &json!({ "node_path": path, "event_name": "click" }),
        )
        .expect("the press was refused")
}

#[test]
fn counting_up_a_field_of_an_entry_reaches_the_file() {
    serialised(|| {
        let root = a_root("inc");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alpha", "bump");
        assert!(
            on_disk(&root).contains("- done = 1"),
            "the press answered Ok and wrote nothing:\n{}",
            on_disk(&root)
        );
        assert_eq!(overseer::undo::depth(&root, "t.os"), 1, "no step to take it back");
    });
}

#[test]
fn counting_up_twice_counts_to_two() {
    // It has to read back what it wrote. Recording the override without clearing what says the
    // value came from the template would make every press the first one.
    serialised(|| {
        let root = a_root("twice");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alpha", "bump");
        press(&service, "alpha", "bump");
        assert!(on_disk(&root).contains("- done = 2"), "{}", on_disk(&root));
    });
}

#[test]
fn pressing_on_two_different_entries_writes_to_both() {
    // The sequence that was reported: did it on one task, then on another.
    serialised(|| {
        let root = a_root("two");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alpha", "bump");
        press(&service, "beta", "bump");
        let text = on_disk(&root);
        assert_eq!(
            text.matches("- done = 1").count(),
            2,
            "only one of the two landed:\n{}",
            text
        );
    });
}

#[test]
fn setting_and_toggling_still_reach_it() {
    // These two always worked. Here so that fixing `inc` by breaking them is not a pass.
    serialised(|| {
        let root = a_root("others");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alpha", "mark");
        press(&service, "alpha", "flip");
        let text = on_disk(&root);
        assert!(text.contains("- done = 7"), "set did not land:\n{}", text);
        assert!(text.contains("- flag = true"), "toggle did not land:\n{}", text);
    });
}

#[test]
fn the_answer_says_whether_the_file_moved() {
    // The page holds the text the file had when it last heard, and refuses to save over a
    // document something else has written since. A write made here moves the file without the
    // page sending anything, so the answer has to say so - otherwise that baseline is stale from
    // the first change onwards, which is what had every save after it refused.
    serialised(|| {
        let root = a_root("wrote");
        let service = DocumentRoot::new(&root).expect("open the root");
        let answer = press(&service, "alpha", "bump");
        assert_eq!(answer.get("wrote").and_then(|w| w.as_bool()), Some(true));
        // Nothing guarded was involved, so what the page is shown is what the file says, and
        // sending it a second time would be the document twice over.
        assert!(answer.get("file_text").map_or(true, |f| f.is_null()));
    });
}

#[test]
fn a_press_that_moves_only_the_viewer_says_the_file_did_not_move() {
    // Then the page's baseline still stands. Saying otherwise would have it adopt a text the
    // file does not have, and the next save would be refused for disagreeing with the file.
    serialised(|| {
        let root = a_root("viewer");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        let answer = press(&service, "alpha", "back");
        assert_eq!(on_disk(&root), before, "the viewer's day reached the file");
        assert_eq!(answer.get("wrote").and_then(|w| w.as_bool()), Some(false));
    });
}

#[test]
fn a_press_that_moves_both_says_what_the_file_holds() {
    // The viewer's field is taken out of what gets written, so the file and the page differ -
    // and the page needs the file's version as the baseline it will be checked against, not its
    // own.
    serialised(|| {
        let root = a_root("both");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alpha", "back");
        let answer = press(&service, "alpha", "bump");
        assert_eq!(answer.get("wrote").and_then(|w| w.as_bool()), Some(true));
        let said = answer
            .get("file_text")
            .and_then(|f| f.as_str())
            .expect("it did not say what the file holds");
        assert_eq!(
            said,
            on_disk(&root),
            "what it said the file holds is not what the file holds"
        );
    });
}
