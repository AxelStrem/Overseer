//! A card dragged on a canvas gives its entry a new `x` and `y` at once: one instruction writes
//! both fields, runs each field's `on change`, and is one step to take back.

use crate::common::serialised;
use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab board (label="Board", mutable=true) {

    int moves = 0

    div (hidden=true) {
        div Card {
            string title = ""
            float x = 0 {
                on change {
                    inc (path="/board/moves")
                }
            }
            float y = 0 {
                on change {
                    inc (path="/board/moves")
                }
            }
        }
    }

    list Notes (entry=<Card>, key="title", view="canvas", x="x", y="y") {
        - {
            - title = "placed"
            - x = 120
            - y = 40
        }
        - {
            - title = "loose"
        }
    }
}
"#;



fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_canvas_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("d.os")).expect("read")
}

fn place(service: &DocumentRoot, entry: &str, x: f64, y: f64) -> serde_json::Value {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("board/Notes/[{}]", entry))
        .unwrap_or_else(|| panic!("no entry {}", entry));
    service
        .command_for(
            "alice",
            Some("d.os"),
            "set_overseer_entry_fields",
            &json!({
                "entry_path": path,
                "settings": [
                    { "node_path": ["x"], "value": { "Float": x } },
                    { "node_path": ["y"], "value": { "Float": y } },
                ],
            }),
        )
        .expect("the placing was refused")
}

#[test]
fn both_fields_are_written_and_both_handlers_run() {
    serialised(|| {
        let root = a_root("both");
        let service = DocumentRoot::new(&root).expect("open the root");
        place(&service, "placed", 300.0, 90.0);
        let text = on_disk(&root);
        assert!(text.contains("- x = 300"), "{}", text);
        assert!(text.contains("- y = 90"), "{}", text);
        assert!(text.contains("int moves = 2"), "{}", text);
    });
}

#[test]
fn an_entry_without_a_place_takes_one() {
    serialised(|| {
        let root = a_root("loose");
        let service = DocumentRoot::new(&root).expect("open the root");
        place(&service, "loose", 10.0, 20.0);
        let text = on_disk(&root);
        let loose = &text[text.find(r#"- title = "loose""#).expect("the entry")..];
        assert!(loose.contains("- x = 10"), "{}", text);
        assert!(loose.contains("- y = 20"), "{}", text);
    });
}

#[test]
fn a_field_that_keeps_its_value_runs_nothing() {
    serialised(|| {
        let root = a_root("kept");
        let service = DocumentRoot::new(&root).expect("open the root");
        place(&service, "placed", 120.0, 75.0);
        assert!(on_disk(&root).contains("int moves = 1"), "{}", on_disk(&root));
    });
}

#[test]
fn one_undo_takes_all_of_it_back() {
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        place(&service, "placed", 300.0, 90.0);
        assert_eq!(overseer::undo::depth(&root, "d.os"), 1, "not one step");
        service.undo("d.os").expect("undo");
        assert_eq!(on_disk(&root), DOCUMENT);
    });
}
