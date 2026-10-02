//! An action that picks an entry by a key two entries hold is refused, rather than given the first.
//!
//! Remove, set_in_list and move each found the entry by its key and took the first that held it.
//! With the key unique that is the entry; with two holding it, the press was about the other one
//! as often as not. tpsc.os had four project tasks sharing an `added` stamp, and `finish` on the
//! fourth would have recorded the fourth as finished and taken the first off the list - gone from
//! the plan and recorded nowhere. A refused press writes nothing, so all it costs is the press.

use overseer::server::DocumentRoot;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string id = ""
            int qty = 0
        }
    }

    list Rows (entry=<Row>, key="id") {
        - {
            - id = "same"
            - qty = 1
        }
        - {
            - id = "same"
            - qty = 2
        }
        - {
            - id = "alone"
            - qty = 3
        }
    }

    list Elsewhere (entry=<Row>, key="id") {
    }

    button take_same (label="take") {
        on click {
            remove (from="/t/Rows", keyField="id", keyValue="same")
        }
    }

    button set_same (label="set") {
        on click {
            set_in_list (list="/t/Rows", keyField="id", keyValue="same", field="qty", value=9)
        }
    }

    button move_same (label="move") {
        on click {
            move (from="/t/Rows", to="/t/Elsewhere", keyField="id", keyValue="same")
        }
    }

    button take_alone (label="take") {
        on click {
            remove (from="/t/Rows", keyField="id", keyValue="alone")
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_shared_key_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
    root
}

fn press(button: &str) -> (Result<(), String>, String) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root(button);
    overseer::app_api::forget_baseline();
    let service = DocumentRoot::new(&root).unwrap();
    let outcome = service.run_event("d.os", &format!("t/{}", button), "click").map(|_| ()).map_err(|e| format!("{:?}", e));
    (outcome, std::fs::read_to_string(root.join("d.os")).unwrap())
}

#[test]
fn removing_by_a_key_two_entries_hold_is_refused() {
    let (outcome, after) = press("take_same");
    assert!(outcome.is_err(), "one of the two was removed");
    assert_eq!(after, DOCUMENT, "the refused press wrote something");
}

#[test]
fn changing_by_a_key_two_entries_hold_is_refused() {
    let (outcome, after) = press("set_same");
    assert!(outcome.is_err(), "one of the two was changed");
    assert_eq!(after, DOCUMENT);
}

#[test]
fn moving_by_a_key_two_entries_hold_is_refused() {
    let (outcome, after) = press("move_same");
    assert!(outcome.is_err(), "one of the two was moved");
    assert_eq!(after, DOCUMENT);
}

#[test]
fn a_key_one_entry_holds_still_works() {
    let (outcome, after) = press("take_alone");
    assert!(outcome.is_ok(), "{:?}", outcome.err());
    // The entries' own lines: the buttons name the keys too.
    assert!(!after.contains("- id = \"alone\""), "the entry was not removed:\n{}", after);
    assert_eq!(after.matches("- id = \"same\"").count(), 2, "something else went with it");
}
