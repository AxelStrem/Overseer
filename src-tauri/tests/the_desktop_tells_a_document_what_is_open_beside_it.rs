//! The desktop app tells a document drawn in the drawer which one is open beside it, as
//! `host/open`, the way the server does: its path and name, `edit` where the file is there and
//! nothing where it is not, and whether a write to it is there to take back.

use overseer::app_api;
use overseer::environment::beside;
use overseer::types::{OverseerNode, OverseerValue};
use std::path::PathBuf;

const DRAWER: &str = r#"tab drawer {
    string open_path = $(host/open/path)
    string open_name = $(host/open/name)
    string open_label = $(host/open/label)
    string open_can = $(host/open/can)
    bool undoable = $(host/open/undoable)
}
"#;

const NOTES: &str = "tab notes (mutable=true) {\n    string note = \"\"\n}\n";

const JOURNAL: &str = "tab journal (label=\"My journal\") {\n    string entry = \"\"\n}\n";

fn folder(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer-desktop-open-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("drawer.os"), DRAWER).unwrap();
    std::fs::write(root.join("notes.os"), NOTES).unwrap();
    std::fs::write(root.join("journal.os"), JOURNAL).unwrap();
    root
}

fn slashed(path: PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes.iter().find_map(|n| {
        if n.name == name && n.node_type != "context" {
            Some(n)
        } else {
            find(&n.children, name)
        }
    })
}

fn value(nodes: &[OverseerNode], name: &str) -> OverseerValue {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no {}", name));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("{} has no value", name))
}

fn text(nodes: &[OverseerNode], name: &str) -> String {
    match value(nodes, name) {
        OverseerValue::String(s) => s,
        other => panic!("{} is {:?}", name, other),
    }
}

fn truth(nodes: &[OverseerNode], name: &str) -> bool {
    match value(nodes, name) {
        OverseerValue::Boolean(b) => b,
        other => panic!("{} is {:?}", name, other),
    }
}

/// The drawer, worked out as the window does with `open` beside it.
fn drawn_beside(drawer: &str, open: &str) -> Vec<OverseerNode> {
    let content = std::fs::read_to_string(drawer).unwrap();
    beside(Some(app_api::open_beside(open)), || {
        app_api::load_document_for(content, drawer, "desktop").unwrap()
    })
}

#[test]
fn a_file_open_beside_the_drawer_is_described_to_it() {
    let root = folder("described");
    let drawer = slashed(root.join("drawer.os"));
    let notes = slashed(root.join("notes.os"));

    let nodes = drawn_beside(&drawer, &notes);
    assert_eq!(text(&nodes, "open_path"), notes);
    assert_eq!(text(&nodes, "open_name"), "notes");
    assert_eq!(text(&nodes, "open_label"), "notes", "no labelled tab: its file's name");
    assert_eq!(text(&nodes, "open_can"), "edit");
    assert!(!truth(&nodes, "undoable"));

    let nodes = drawn_beside(&drawer, &slashed(root.join("journal.os")));
    assert_eq!(text(&nodes, "open_name"), "journal");
    assert_eq!(text(&nodes, "open_label"), "My journal");

    let missing = slashed(root.join("missing.os"));
    let nodes = drawn_beside(&drawer, &missing);
    assert_eq!(text(&nodes, "open_name"), "missing");
    assert_eq!(text(&nodes, "open_label"), "missing");
    assert_eq!(text(&nodes, "open_can"), "", "a file that is not there is not the app's to open");
}

#[test]
fn a_write_to_the_file_open_beside_makes_it_undoable_and_undo_takes_that_back() {
    let root = folder("undo");
    let drawer = slashed(root.join("drawer.os"));
    let notes = slashed(root.join("notes.os"));

    assert!(!app_api::open_beside(&notes).undoable);
    app_api::write_file_keeping_a_step_back(&notes, &NOTES.replace("\"\"", "\"written\"")).unwrap();
    assert!(app_api::open_beside(&notes).undoable);
    assert!(truth(&drawn_beside(&drawer, &notes), "undoable"), "the drawer is told so, not handed what it was told before");

    app_api::undo_document(&notes).unwrap();
    assert!(!app_api::open_beside(&notes).undoable, "once taken back, nothing is");
    assert!(!truth(&drawn_beside(&drawer, &notes), "undoable"));
}
