//! A double click is an event of its own, `on dblclick`, run the way a click is.
//!
//! The page tells it apart; on this side it is one more event: its actions, from the node's own
//! place, as one change and one step to take back, and a click never runs it. A project tracker's
//! rows say it, and hand the page a link to the row to copy.

use crate::common::serial;
use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab board (label="Board", mutable=true) {

    div (hidden=true) {

        div Cell {
            string id = ""
            bool flagged = false
            bool open = false

            on click {
                toggle (path="./flagged")
            }

            on dblclick {
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

const PROJECT: &str = include_str!("../../../examples/projects/project.os");

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = serial();
    let settings = std::env::temp_dir().join(format!("overseer_dblclick_settings_{}.os", std::process::id()));
    let out = overseer::environment::with_file(settings, body);
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

fn a_root(tag: &str, name: &str, text: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_dblclick_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join(name), text).expect("write");
    app_api::forget_baseline();
    app_api::forget_dependencies();
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
fn a_double_click_runs_on_dblclick_and_not_on_click() {
    serialised(|| {
        let root = a_root("dbl", "b.os", DOCUMENT);
        let service = DocumentRoot::new(&root).expect("open the root");
        send(&service, "a", "dblclick");
        let text = on_disk(&root);
        let cell = cell_in(&text, "a");
        assert!(cell.contains("- open = true"), "the double click did not run:\n{}", text);
        assert!(!cell.contains("flagged"), "the double click ran the click as well:\n{}", text);
        assert!(!cell_in(&text, "b").contains("open"), "the double click reached another cell:\n{}", text);
    });
}

#[test]
fn a_click_does_not_run_on_dblclick() {
    serialised(|| {
        let root = a_root("click", "b.os", DOCUMENT);
        let service = DocumentRoot::new(&root).expect("open the root");
        send(&service, "a", "click");
        let cell = cell_in(&on_disk(&root), "a");
        assert!(cell.contains("- flagged = true"), "the click did not run:\n{}", cell);
        assert!(!cell.contains("open"), "the click ran the double click as well:\n{}", cell);
    });
}

#[test]
fn a_double_click_is_one_step_to_take_back() {
    serialised(|| {
        let root = a_root("undo", "b.os", DOCUMENT);
        let service = DocumentRoot::new(&root).expect("open the root");
        send(&service, "a", "dblclick");
        assert_eq!(overseer::undo::depth(&root, "b.os"), 1, "one double click, one step to take back");
        let before = overseer::undo::take(&root, "b.os").expect("a step to take back");
        assert_eq!(before, DOCUMENT, "taking the double click back did not give the document as it was");
    });
}

#[test]
fn a_tracker_row_double_clicked_asks_the_page_to_copy_its_link() {
    serialised(|| {
        let root = a_root("row", "p.os", PROJECT);
        let file = root.join("p.os").to_string_lossy().to_string();
        let nodes = app_api::load_document(PROJECT.to_string()).expect("open");
        for address in ["project/Items/[parser]", "project/History/[escape]"] {
            let path = overseer::addressing::name_path(&nodes, address).unwrap_or_else(|| panic!("nothing at {}", address));
            let answer = app_api::run_event_at(&file, "p.os", path, "dblclick".into(), "s", Vec::new(), Vec::new())
                .expect("the double click was refused");
            assert!(!answer.wrote, "copying a link wrote the document");
            assert_eq!(answer.calls.len(), 1, "{:?}", answer.calls);
            assert_eq!(answer.calls[0].name, "copy");
            assert_eq!(answer.calls[0].args["link"], json!(address), "{:?}", answer.calls[0]);
        }
        assert_eq!(std::fs::read_to_string(&file).unwrap(), PROJECT, "the file changed");
    });
}
