//! A press held down is a hold, and runs `on hold` rather than `on click`.
//!
//! The page tells the two apart; on this side `hold` is one more event, run the way a click is:
//! its actions, from the node's own place, as one change and one step to take back. A cell that
//! says both keeps them apart - a hold never runs the click, and a click never runs the hold.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab board (label="Board", mutable=true) {

    div (hidden=true) {

        div Cell (hold="2s") {
            string id = ""
            bool flagged = false
            bool open = false

            on click {
                toggle (path="./flagged")
            }

            on hold {
                set (path="./open") = true
            }
        }
    }

    list Cells (entry=<Cell>, key="id") {
        - {
            - id = "a"
        }
        - {
            - id = "b"
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
    let root = std::env::temp_dir().join(format!("overseer_hold_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("b.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("b.os")).expect("read")
}

fn send(service: &DocumentRoot, id: &str, event: &str) {
    let nodes = service.open("b.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("board/Cells/[{}]", id))
        .unwrap_or_else(|| panic!("no cell {}", id));
    service
        .command_for("alice", Some("b.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": event }))
        .expect("the press was refused");
}

fn cell_in(text: &str, id: &str) -> String {
    let at = text.find(&format!("- id = \"{}\"", id)).unwrap_or_else(|| panic!("no cell {}:\n{}", id, text));
    let end = text[at..].find('}').map(|e| at + e).unwrap();
    text[at..end].to_string()
}

#[test]
fn a_hold_runs_on_hold_and_not_on_click() {
    serialised(|| {
        let root = a_root("hold");
        let service = DocumentRoot::new(&root).expect("open the root");
        send(&service, "a", "hold");
        let text = on_disk(&root);
        let cell = cell_in(&text, "a");
        assert!(cell.contains("- open = true"), "the hold did not run:\n{}", text);
        assert!(!cell.contains("flagged"), "the hold ran the click as well:\n{}", text);
        assert!(!cell_in(&text, "b").contains("open"), "the hold reached another cell:\n{}", text);
    });
}

#[test]
fn a_click_runs_on_click_and_not_on_hold() {
    serialised(|| {
        let root = a_root("click");
        let service = DocumentRoot::new(&root).expect("open the root");
        send(&service, "a", "click");
        let cell = cell_in(&on_disk(&root), "a");
        assert!(cell.contains("- flagged = true"), "the click did not run:\n{}", cell);
        assert!(!cell.contains("open"), "the click ran the hold as well:\n{}", cell);
    });
}

#[test]
fn a_hold_is_one_step_to_take_back() {
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        send(&service, "a", "hold");
        assert_eq!(overseer::undo::depth(&root, "b.os"), 1, "one hold, one step to take back");
        let before = overseer::undo::take(&root, "b.os").expect("a step to take back");
        assert_eq!(before, DOCUMENT, "taking the hold back did not give the document as it was");
    });
}
