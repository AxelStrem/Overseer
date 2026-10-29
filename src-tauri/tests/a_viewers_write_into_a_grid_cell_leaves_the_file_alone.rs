//! A grid's cell written for the viewer - a guarded field set by a press - is the viewer's alone:
//! the file keeps the grid as it was written, with no `- A1 { }` left behind for the cell. That
//! includes a later press that changes nothing at all, which once wrote the file anyway and left
//! an empty block for every cell the viewer had set, and a press that really writes the file
//! beside one.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOC: &str = r#"div (hidden=true) {
    div Cell {
        bool opened = false
        bool flagged = false

        on click {
            toggle (path="./flagged")
        }

        on hold {
            press (path="./open")
        }

        button open {
            on click {
                set (path="../opened") = true
            }
        }
    }
}

div tally (mutable=true) {
    int presses = 0
}

div game (mutable="guarded") {
    grid Field (entry=<Cell>, columns=3, rows=2) { }

    button open_all {
        on click {
            each (in=$(/game/Field.cells()), as="c") {
                press (path="c/open")
            }
        }
    }

    button nothing {
        on click {
            if (cond=$(false)) {
                set (path="/game/Field/A1/opened") = false
            }
        }
    }

    button first {
        on click {
            set (path="/game/Field/A1/opened") = true
        }
    }

    button count {
        on click {
            inc (path="/tally/presses")
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_gridview_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), DOC).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn press(service: &DocumentRoot, address: &str) {
    send(service, address, "click")
}

fn send(service: &DocumentRoot, address: &str, event: &str) {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, address)
        .unwrap_or_else(|| panic!("no such button: {}", address));
    service
        .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": event }))
        .unwrap_or_else(|e| panic!("{} was refused: {:?}", address, e));
}

fn set_for(session: &str, field: &str) -> usize {
    overseer::viewstate::overlay(session, "d.os")
        .iter()
        .filter(|(address, value)| {
            address.ends_with(&format!("/{}", field)) && matches!(value, overseer::types::OverseerValue::Boolean(true))
        })
        .count()
}

#[test]
fn a_set_into_a_cell_is_the_viewers() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("set");
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "game/first");
    assert_eq!(set_for("someone", "opened"), 1);
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), DOC, "the press wrote the file");
}

#[test]
fn every_cell_pressed_open_is_the_viewers() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("each");
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "game/open_all");
    assert_eq!(set_for("someone", "opened"), 6);
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), DOC, "the press wrote the file");
}

#[test]
fn a_tap_and_a_hold_on_a_cell_are_the_viewers() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("hold");
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "game/Field/B1");
    send(&service, "game/Field/C2", "hold");
    assert_eq!(set_for("someone", "flagged"), 1);
    assert_eq!(set_for("someone", "opened"), 1);
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), DOC, "the press wrote the file");
}

#[test]
fn a_press_that_changes_nothing_after_one_into_a_cell_leaves_the_file_alone() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("nothing");
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "game/first");
    press(&service, "game/nothing");
    assert_eq!(set_for("someone", "opened"), 1);
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), DOC, "the press wrote the file");
}

#[test]
fn a_real_write_beside_a_cell_set_for_the_viewer_writes_only_itself() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("real");
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "game/first");
    press(&service, "game/count");
    assert_eq!(set_for("someone", "opened"), 1);
    assert_eq!(
        std::fs::read_to_string(root.join("d.os")).unwrap(),
        DOC.replace("int presses = 0", "int presses = 1"),
        "the file holds more than the count"
    );
}

#[test]
fn a_cell_the_file_wrote_keeps_its_block_when_the_viewer_sets_another_of_its_fields() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("kept");
    let written = DOC.replace(
        "grid Field (entry=<Cell>, columns=3, rows=2) { }",
        "grid Field (entry=<Cell>, columns=3, rows=2) {
        - A1 {
            - flagged = true
        }
    }",
    );
    std::fs::write(root.join("d.os"), &written).expect("write");
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "game/first");
    press(&service, "game/count");
    assert_eq!(set_for("someone", "opened"), 1);
    assert_eq!(
        std::fs::read_to_string(root.join("d.os")).unwrap(),
        written.replace("int presses = 0", "int presses = 1"),
        "the file lost the cell's own field, or gained the viewer's"
    );
}
