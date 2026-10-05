//! A move between lists, a move within one, and a sort are written to the file.
//!
//! Each of them only rearranges entries that were all read from the text, so every entry still
//! matches its source. Unless the list says it has changed, the serializer replays it from the
//! text it was read from, and the press answers ok while the file stays exactly as it was.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
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
            - id = "a"
            - qty = 1
        }
        - {
            - id = "b"
            - qty = 2
        }
    }

    list Elsewhere (entry=<Row>, key="id") {
        - {
            - id = "c"
            - qty = 3
        }
    }

    button across (label="m") {
        on click {
            move (from="/t/Elsewhere", to="/t/Rows", keyField="id", keyValue="c")
        }
    }

    button within (label="m") {
        on click {
            move (from="/t/Rows", to="/t/Rows", keyField="id", keyValue="b", at=0)
        }
    }

    button sorted (label="s") {
        on click {
            sort (list="/t/Rows", by=$(x/qty), order="desc")
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn press(button: &str) -> String {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("overseer_move_writes_{}_{}", button, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
    app_api::forget_baseline();
    let service = DocumentRoot::new(&root).unwrap();
    service.run_event("d.os", &format!("t/{}", button), "click").unwrap();
    let after = std::fs::read_to_string(root.join("d.os")).unwrap();
    // What was written reads back as itself: the change is not one a later save would undo or
    // reformat.
    let nodes = DocumentManager::with_document(Some(root.clone()), || app_api::load_document(after.clone())).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), after, "a second save of {} changed the file", button);
    after
}

/// The ids in a list, in the order the file holds them.
fn ids(file: &str, list: &str) -> Vec<String> {
    let start = file.find(&format!("list {} ", list)).unwrap();
    let body = &file[start..];
    let end = body.find("\n    }\n").unwrap();
    body[..end]
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- id = \""))
        .map(|rest| rest.trim_end_matches('"').to_string())
        .collect()
}

#[test]
fn a_move_into_another_list_leaves_the_one_and_reaches_the_other() {
    let after = press("across");
    assert_eq!(ids(&after, "Rows"), ["a", "b", "c"], "{}", after);
    assert!(ids(&after, "Elsewhere").is_empty(), "{}", after);
}

#[test]
fn a_move_within_a_list_reorders_it() {
    let after = press("within");
    assert_eq!(ids(&after, "Rows"), ["b", "a"], "{}", after);
}

#[test]
fn a_sort_reorders_the_list() {
    let after = press("sorted");
    assert_eq!(ids(&after, "Rows"), ["b", "a"], "{}", after);
}
