//! An enum with an `on change`, in an unnamed div of a list entry, writes its own value.
//!
//! The page sends the value picked with the field's handler, as one change. Seen once with the
//! field in a row of a board's cards: the handler's write reached the file and the value did not,
//! so the card went back to its column on reload. Both are written, and one undo takes both back.

use overseer::app_api;
use overseer::types::*;

const DOCUMENT: &str = r#"div (hidden=true) {
    div Standing {
        string tag = ""
    }
    div Feature (layout="vertical") {
        div (layout="horizontal") {
            string handle (label="") = ""
            enum standing (vocabulary="/features/Standings") = "proposed" {
                on change {
                    set (path="../touched_at", mode="value") = $(now())
                }
            }
        }
        timestamp touched_at (hidden=true) = "2026-01-01T00:00:00Z"
    }
}
div features (mutable=true) {
    list Standings (entry=<Standing>, key="tag") {
        - {
            - tag = "proposed"
        }
        - {
            - tag = "wanted"
        }
    }
    list Features (entry=<Feature>, key="handle", view="board", by="standing") {
        - {
            - handle = "X1"
        }
    }
}
"#;

fn a_document() -> (std::path::PathBuf, String) {
    let root = std::env::temp_dir().join(format!("overseer_enum_in_a_row_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("f.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    (root, file.to_string_lossy().to_string())
}

fn on_disk(file: &str) -> String {
    std::fs::read_to_string(file).unwrap()
}

#[test]
fn a_pick_writes_the_value_and_what_its_handler_does_as_one_step() {
    let (root, file) = a_document();
    let nodes = app_api::load_document(on_disk(&file)).expect("open");
    let path = overseer::addressing::name_path(&nodes, "features/Features/[X1]/standing").expect("the field");
    let steps = overseer::undo::depth(&root, "f.os");
    let value = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String("wanted".into()) };
    app_api::run_event_at(&file, "f.os", path, "change".into(), "s", Vec::new(), vec![value])
        .expect("the pick was refused");

    let text = on_disk(&file);
    assert!(text.contains("- standing = \"wanted\""), "the value was not written:\n{}", text);
    assert!(text.contains("- touched_at = "), "the handler's write is missing:\n{}", text);
    assert_eq!(overseer::undo::depth(&root, "f.os"), steps + 1, "a pick was more than one step");

    app_api::undo_document(&file).expect("undo");
    assert_eq!(on_disk(&file), DOCUMENT, "one undo did not take the pick back whole");
}
