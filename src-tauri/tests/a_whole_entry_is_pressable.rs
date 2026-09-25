//! A whole entry can be the thing you press.
//!
//! A button is its own node, so pressing one means hitting it - and on a phone, ticking a
//! shopping item off meant hitting a small button at the end of a row that is mostly not the
//! button. A div that says `on click` is pressed wherever it is touched.
//!
//! Nothing on this side knew a press came from a button: the owner of an `on click` is found by
//! its path and can be any node. What was missing was the page drawing a div as pressable. These
//! pin the part that was already true, so the page can rely on it: a div's actions run, from the
//! div's own place, which is one level down from where a button inside it would stand - so a bare
//! `$(x)` reaches the entry's own field where a button says `$(../x)`.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab shopping (label="Shopping", mutable=true) {

    div (hidden=true) {

        div Item (layout="horizontal") {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string handle = ""
            float amount = 1

            on click {
                append (list="/shopping/History") {
                    - handle = $(handle)
                    - amount = $(amount)
                }
                remove (from="/shopping/List", keyField="added", keyValue=$(added))
            }
        }

        div Bought (layout="horizontal") {
            string handle = ""
            float amount = 1
        }
    }

    list List (entry=<Item>, key="added") {
        - {
            - added = "2026-08-11T09:00:00Z"
            - handle = "milk"
            - amount = 2
        }
        - {
            - added = "2026-08-12T09:00:00Z"
            - handle = "bread"
        }
    }

    list History (entry=<Bought>) {
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
    let root = std::env::temp_dir().join(format!("overseer_pressable_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("s.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("s.os")).expect("read")
}

/// Press the entry itself - not anything inside it.
fn press_the_entry(service: &DocumentRoot, added: &str) -> serde_json::Value {
    let nodes = service.open("s.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("shopping/List/[{}]", added))
        .unwrap_or_else(|| panic!("no entry keyed {}", added));
    service
        .command_for(
            "alice",
            Some("s.os"),
            "run_overseer_event",
            &json!({ "node_path": path, "event_name": "click" }),
        )
        .expect("the press was refused")
}

/// What lies between `list <name>` and the brace that closes it.
fn body_of<'a>(text: &'a str, list: &str) -> &'a str {
    let at = text.find(&format!("list {} ", list)).expect("no such list");
    let rest = &text[at..];
    let end = rest.find("\n    }").expect("the list never closes");
    &rest[..end]
}

#[test]
fn pressing_an_entry_runs_what_it_says() {
    serialised(|| {
        let root = a_root("runs");
        let service = DocumentRoot::new(&root).expect("open the root");
        press_the_entry(&service, "2026-08-11T09:00:00Z");
        let text = on_disk(&root);
        let list = body_of(&text, "List");
        let history = body_of(&text, "History");
        assert!(!list.contains("\"milk\""), "it was not taken off the list:\n{}", text);
        assert!(list.contains("\"bread\""), "the wrong entry went:\n{}", text);
        assert!(history.contains("- handle = \"milk\""), "it did not reach the history:\n{}", text);
        assert_eq!(overseer::undo::depth(&root, "s.os"), 1, "one press, one step to take back");
        // The handler is the template's. Written into the entry that is left, every press would
        // grow the file by a copy of it.
        assert!(!list.contains("on click"), "the handler was copied into the entries:\n{}", text);
    });
}

#[test]
fn an_entrys_actions_read_its_own_fields() {
    // From the entry's own place, so `amount` is this entry's amount - two for milk - and not
    // the template's one, nor another entry's.
    serialised(|| {
        let root = a_root("own");
        let service = DocumentRoot::new(&root).expect("open the root");
        press_the_entry(&service, "2026-08-11T09:00:00Z");
        let text = on_disk(&root);
        let history = body_of(&text, "History");
        assert!(history.contains("- amount = 2"), "it read some other amount:\n{}", text);
    });
}

#[test]
fn pressing_the_second_entry_takes_the_second() {
    serialised(|| {
        let root = a_root("second");
        let service = DocumentRoot::new(&root).expect("open the root");
        press_the_entry(&service, "2026-08-12T09:00:00Z");
        let text = on_disk(&root);
        assert!(body_of(&text, "List").contains("\"milk\""), "{}", text);
        assert!(body_of(&text, "History").contains("- handle = \"bread\""), "{}", text);
    });
}
