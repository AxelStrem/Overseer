//! A grid holds `columns` x `rows` cells, each made from its `entry=` and named spreadsheet-style
//! (`A1`, `B1`, ... `C5`). What its block writes overrides only the cell it names, and only that
//! is saved: a cell set since is written into the block as `- C5 { ... }`, among the others in the order
//! of their places.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::*;

const DOC: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Cell {
            int value = 0
            string note = ""
        }
    }

    grid Board (entry=<Cell>, columns=3, rows=2) {
        - B2 {
            - value = 7
        }
    }

    grid Scores (entry=int, columns=2, rows=2) {
        - A2 = 4
    }

    int total = $(Board.cells().sum(|c| c/value) + Scores.cells().sum())
    int corner = $(Board.at(1, 1)/value)
    int second_row = $(Board.row(1).sum(|c| c/value))
    int first_column = $(Scores.column(0).sum())
    int cells = $(Board.cells().count())

    button mark (label="mark") {
        on click {
            set (path="../Board[2,0]/value") = 5
        }
    }
    button note (label="note") {
        on click {
            set (path="../Board/A2/note") = "here"
        }
    }
    button score (label="score") {
        on click {
            set (path="../Scores[1,1]") = 9
        }
    }
}
"#;

/// Saving consults a process-wide registry of parsed sources, so two of these running at once
/// see each other's document.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn value_at(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let node = overseer::addressing::find(nodes, address)?;
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

fn set(nodes: &mut Vec<OverseerNode>, address: &str, value: OverseerValue) {
    let path = overseer::addressing::name_path(nodes, address).expect("a path to the cell");
    overseer::actions::ActionExecutor::assign_value(nodes, &format!("/{}", path.join("/")), value)
        .expect("set the value");
}

#[test]
fn every_cell_is_made_and_named_by_its_place() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    let board = overseer::addressing::find(&nodes, "t/Board").expect("the grid");
    let names: Vec<&str> = board.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["A1", "B1", "C1", "A2", "B2", "C2"]);
    assert_eq!(value_at(&nodes, "t/Board/B2/value"), Some(OverseerValue::Integer(7)));
    assert_eq!(value_at(&nodes, "t/Board/C2/value"), Some(OverseerValue::Integer(0)));
    assert_eq!(value_at(&nodes, "t/Scores/A2"), Some(OverseerValue::Integer(4)));
    assert_eq!(value_at(&nodes, "t/Scores/B1"), Some(OverseerValue::Integer(0)));
}

#[test]
fn an_untouched_grid_saves_as_it_was_written() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DOC);
}

#[test]
fn a_cell_set_since_is_written_into_the_block() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut nodes = app_api::load_document(DOC.to_string()).unwrap();
    set(&mut nodes, "t/Board/C1/value", OverseerValue::Integer(5));
    set(&mut nodes, "t/Board/B2/note", OverseerValue::String("x".into()));
    set(&mut nodes, "t/Scores/B2", OverseerValue::Integer(9));
    let saved = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
    let expected = DOC
        .replace(
            "        - B2 {\n            - value = 7\n        }\n",
            "        - C1 {\n            - value = 5\n        }\n        - B2 {\n            - value = 7\n            - note = \"x\"\n        }\n",
        )
        .replace("        - A2 = 4\n", "        - A2 = 4\n        - B2 = 9\n");
    assert_eq!(saved, expected);
    let reread = app_api::load_document(saved.clone()).unwrap();
    assert_eq!(value_at(&reread, "t/Board/C1/value"), Some(OverseerValue::Integer(5)));
    assert_eq!(value_at(&reread, "t/Scores/B2"), Some(OverseerValue::Integer(9)));
    assert_eq!(OverseerFileHandler::serialize_nodes(&reread).unwrap(), saved);
}

#[test]
fn a_formula_reads_cells_by_place_row_and_column() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(value_at(&nodes, "t/total"), Some(OverseerValue::Integer(11)));
    assert_eq!(value_at(&nodes, "t/corner"), Some(OverseerValue::Integer(7)));
    assert_eq!(value_at(&nodes, "t/second_row"), Some(OverseerValue::Integer(7)));
    assert_eq!(value_at(&nodes, "t/first_column"), Some(OverseerValue::Integer(4)));
    assert_eq!(value_at(&nodes, "t/cells"), Some(OverseerValue::Integer(6)));
}

#[test]
fn a_cell_written_outside_the_grid_is_kept_and_said() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = DOC.replace("        - A2 = 4\n", "        - A2 = 4\n        - C3 = 1\n");
    let nodes = app_api::load_document(doc.clone()).unwrap();
    let scores = overseer::addressing::find(&nodes, "t/Scores").expect("the grid");
    let why = scores.parameters.get("_inherit_error").expect("a problem said");
    assert!(matches!(why, OverseerValue::String(s) if s.contains("C3")), "{:?}", why);
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), doc);
}

fn press(button: &str, tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_grid_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOC).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let path = file.to_string_lossy().to_string();
    let nodes = app_api::load_document(DOC.to_string()).expect("open");
    let node_path = overseer::addressing::name_path(&nodes, button).expect("the button");
    app_api::run_event_at(&path, "d.os", node_path, "click".into(), "s", Vec::new(), Vec::new())
        .expect("the press was refused");
    std::fs::read_to_string(&file).unwrap()
}

#[test]
fn a_press_names_a_cell_by_its_place() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/mark", "place");
    let expected = DOC.replace(
        "        - B2 {\n",
        "        - C1 {\n            - value = 5\n        }\n        - B2 {\n",
    );
    assert_eq!(written, expected);
}

#[test]
fn a_press_names_a_cell_by_its_name() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/note", "name");
    let expected = DOC.replace(
        "        - B2 {\n",
        "        - A2 {\n            - note = \"here\"\n        }\n        - B2 {\n",
    );
    assert_eq!(written, expected);
}

#[test]
fn a_press_sets_a_cell_of_plain_values() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/score", "plain");
    assert_eq!(written, DOC.replace("        - A2 = 4\n", "        - A2 = 4\n        - B2 = 9\n"));
}

#[test]
fn an_edit_from_the_page_is_written_into_the_block() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut changed = std::collections::HashMap::new();
    changed.insert("t/Board/C2/value".to_string(), OverseerValue::Integer(3));
    changed.insert("t/Scores/B1".to_string(), OverseerValue::Integer(2));
    let nodes = app_api::resolve_selective(
        DOC.to_string(),
        changed.keys().cloned().collect(),
        Some(changed),
    )
    .expect("the edit");
    assert_eq!(value_at(&nodes, "t/total"), Some(OverseerValue::Integer(16)));
    let expected = DOC
        .replace(
            "            - value = 7\n        }\n",
            "            - value = 7\n        }\n        - C2 {\n            - value = 3\n        }\n",
        )
        .replace("        - A2 = 4\n", "        - B1 = 2\n        - A2 = 4\n");
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), expected);
}

#[test]
fn a_cell_name_and_its_place_agree() {
    use overseer::addressing::{cell_name, cell_place};
    assert_eq!(cell_name(0, 0), "A1");
    assert_eq!(cell_name(2, 4), "C5");
    assert_eq!(cell_name(25, 0), "Z1");
    assert_eq!(cell_name(26, 9), "AA10");
    assert_eq!(cell_name(701, 0), "ZZ1");
    assert_eq!(cell_name(702, 0), "AAA1");
    for column in 0..800 {
        assert_eq!(cell_place(&cell_name(column, 3)), Some((column, 3)));
    }
    assert_eq!(cell_place("A0"), None);
    assert_eq!(cell_place("a1"), None);
    assert_eq!(cell_place("A"), None);
    assert_eq!(cell_place("12"), None);
    assert_eq!(cell_place("A1b"), None);
}
