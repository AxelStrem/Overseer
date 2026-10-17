//! A press that takes away the entry its button is in goes on reading that entry as it was.
//!
//! The bucket lists' trash button removes a film from all three lists, one `remove` after another,
//! each keyed by `$(../added)`. Each action works out its arguments when its turn comes, from the
//! button's address, and that address runs through the entry: once the `remove` that found the
//! film had taken it away, the next one read `..` from an address that held nothing - and the
//! press was refused whole, film and all - or, with another entry after it, from the entry that
//! had moved up into its place, and took that one out of the other lists as well.

use overseer::server::DocumentRoot;

const DOCUMENT: &str = r#"tab b (label="B", mutable=true) {
    div (hidden=true) {
        div Thing (layout="vertical") {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            div (layout="horizontal") {
                text name (label="") = ""
                button drop (label="x") {
                    on click {
                        remove (from="/b/All", keyField="added", keyValue=$(../added))
                        remove (from="/b/Planned", keyField="added", keyValue=$(../added))
                        remove (from="/b/Done", keyField="added", keyValue=$(../added))
                        set (path="/b/last") = $(../name)
                    }
                }
            }
        }
    }
    string last = ""
    list All (entry=<Thing>, key="added") {
        - {
            - added = "2026-10-15T13:21:04.048159960+00:00"
            - name = "first"
        }
        - {
            - added = "2026-10-15T13:22:04.048159960+00:00"
            - name = "second"
        }
    }
    list Planned (entry=<Thing>, key="added") {
        - {
            - added = "2026-10-16T13:21:04.048159960+00:00"
            - name = "planned"
        }
        - {
            - added = "2026-10-15T13:22:04.048159960+00:00"
            - name = "second"
        }
    }
    list Done (entry=<Thing>, key="added") {
        - {
            - added = "2026-10-17T13:21:04.048159960+00:00"
            - name = "done"
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Presses the trash button of the entry of `list` at `at`, in a copy of the document, and gives
/// back what the press answered and the file it left.
fn drop_entry(list: &str, at: usize) -> (Result<(), String>, String) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("overseer_press_goes_on_{}_{}_{}", list, at, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
    overseer::app_api::forget_baseline();
    let service = DocumentRoot::new(&root).unwrap();
    let entry = service.read_at("d.os", &format!("b/{}", list)).unwrap().child_addresses[at].clone();
    let outcome = service
        .run_event("d.os", &format!("{}/drop", entry), "click")
        .map(|_| ())
        .map_err(|e| format!("{:?}", e));
    let after = std::fs::read_to_string(root.join("d.os")).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    (outcome, after)
}

fn held(after: &str, name: &str) -> usize {
    after.matches(&format!("- name = \"{}\"", name)).count()
}

#[test]
fn an_entry_of_a_list_removed_from_before_the_last_leaves() {
    let (outcome, after) = drop_entry("Planned", 0);
    assert!(outcome.is_ok(), "the press was refused: {:?}", outcome.err());
    assert_eq!(held(&after, "planned"), 0, "the entry is still there:\n{}", after);
    assert_eq!(held(&after, "second"), 2, "another entry went with it:\n{}", after);
    assert_eq!(held(&after, "first"), 1, "another entry went with it:\n{}", after);
    assert_eq!(held(&after, "done"), 1, "another entry went with it:\n{}", after);
}

#[test]
fn an_entry_in_two_lists_leaves_both_and_takes_nothing_else() {
    let (outcome, after) = drop_entry("All", 1);
    assert!(outcome.is_ok(), "the press was refused: {:?}", outcome.err());
    assert_eq!(held(&after, "second"), 0, "the entry is still there:\n{}", after);
    assert_eq!(held(&after, "first"), 1, "another entry went with it:\n{}", after);
    assert_eq!(held(&after, "planned"), 1, "another entry went with it:\n{}", after);
}

#[test]
fn the_entry_after_the_one_removed_is_not_read_in_its_place() {
    // `first` has `second` after it: once `first` is gone, `second` is where `first` was.
    let (outcome, after) = drop_entry("All", 0);
    assert!(outcome.is_ok(), "the press was refused: {:?}", outcome.err());
    assert_eq!(held(&after, "first"), 0, "the entry is still there:\n{}", after);
    assert_eq!(held(&after, "second"), 2, "the entry after it was taken too:\n{}", after);
    assert!(after.contains("string last = \"first\""), "the press read another entry's name:\n{}", after);
}

#[test]
fn an_entry_of_the_last_list_leaves() {
    let (outcome, after) = drop_entry("Done", 0);
    assert!(outcome.is_ok(), "the press was refused: {:?}", outcome.err());
    assert_eq!(held(&after, "done"), 0, "the entry is still there:\n{}", after);
    assert!(after.contains("string last = \"done\""), "{}", after);
}
