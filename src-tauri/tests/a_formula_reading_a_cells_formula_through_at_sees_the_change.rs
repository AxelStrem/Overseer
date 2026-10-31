//! A formula that reads another cell's formula through `at(...)/field`, or another entry's through
//! `find(...)/field`, sees the change that moved it. It once read a cell that comes later in the
//! document as if it were its own: the followed formula was worked out at the reader's path, so
//! a bare name in it climbed to the reader's cell, and the answer was right only a change later.

use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use serde_json::json;

const DOC: &str = r#"div (hidden=true) {
    div Cell {
        bool on = false
        int v = $(on ? 1 : 0)
        int next_v = $(Row.at((column() + 1) % 3, 0)/v)
        on click {
            toggle (path="./on")
        }
    }

    div Item {
        string k = ""
        bool on = false
        int v = $(on ? 1 : 0)
        int b_v = $(Items.find("b")/v)
        on click {
            toggle (path="./on")
        }
    }
}

div r (mutable=MUTABLE) {
    grid Row (entry=<Cell>, columns=3, rows=1, header=false) { }

    list Items (entry=<Item>, key="k") {
        - {
            - k = "a"
        }
        - {
            - k = "b"
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str, mutable: &str) -> DocumentRoot {
    let root = std::env::temp_dir().join(format!("overseer_gridstale_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), DOC.replace("MUTABLE", mutable)).expect("write");
    overseer::viewstate::forget_all();
    DocumentRoot::new(&root).expect("open the root")
}

fn press(service: &DocumentRoot, address: &str) {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, address)
        .unwrap_or_else(|| panic!("no such node: {}", address));
    service
        .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .unwrap_or_else(|e| panic!("{} was refused: {:?}", address, e));
}

fn value_at(service: &DocumentRoot, path: &str) -> Option<OverseerValue> {
    let nodes: Vec<OverseerNode> = service.open_for("someone", "d.os").expect("open");
    let node = overseer::addressing::node_at_path(&nodes, path)?;
    node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")).cloned()
}

fn a_tap_reaches_the_cell_before(mutable: &str, tag: &str) {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let service = a_root(tag, mutable);
    press(&service, "r/Row/B1");
    assert_eq!(value_at(&service, "r/Row/B1/v"), Some(OverseerValue::Integer(1)));
    assert_eq!(value_at(&service, "r/Row/A1/next_v"), Some(OverseerValue::Integer(1)));
    assert_eq!(value_at(&service, "r/Row/C1/next_v"), Some(OverseerValue::Integer(0)));
}

#[test]
fn a_cell_reading_the_next_cells_formula_sees_its_tap() {
    a_tap_reaches_the_cell_before("true", "grid");
}

#[test]
fn a_cell_reading_the_next_cells_formula_sees_its_tap_for_the_viewer() {
    a_tap_reaches_the_cell_before("\"guarded\"", "guarded");
}

#[test]
fn an_entry_reading_a_later_entrys_formula_through_find_sees_its_change() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let service = a_root("find", "true");
    press(&service, "r/Items/[b]");
    let nodes = service.open("d.os").expect("open");
    let items = overseer::addressing::node_at_path(&nodes, "r/Items").expect("the list");
    let a = items
        .get_accessible_children()
        .into_iter()
        .find(|entry| {
            entry.get_accessible_children().into_iter().any(|f| {
                f.name == "k" && f.parameters.get("value") == Some(&OverseerValue::String("a".into()))
            })
        })
        .expect("entry a");
    let b_v = a.get_accessible_children().into_iter().find(|f| f.name == "b_v").expect("b_v");
    assert_eq!(b_v.parameters.get("_computed_value"), Some(&OverseerValue::Integer(1)));
}
