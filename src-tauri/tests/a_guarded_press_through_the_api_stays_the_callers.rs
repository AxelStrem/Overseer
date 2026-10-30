//! A press through the API on a document holding something for the viewer is the caller's alone,
//! and the press after it starts from the document as the caller last saw it.
//!
//! The first press used to be held for the next under the file's own text, with the caller's
//! values worked into it - so a cell the bot flagged on a guarded board was flagged on the board of
//! everyone who opened it afterwards, while the file never said so. And every press after the first
//! worked the whole document out again, since nothing said where the caller's was held.

use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_api_guarded_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), minesweeper()).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn minesweeper() -> String {
    let at = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/games/minesweeper.os");
    std::fs::read_to_string(at).expect("the example")
}

/// Every value a node holds, written or worked out, by where it is.
fn values(nodes: &[OverseerNode]) -> std::collections::BTreeMap<String, String> {
    fn walk(nodes: &[OverseerNode], trail: &mut Vec<String>, out: &mut std::collections::BTreeMap<String, String>) {
        for (at, node) in nodes.iter().enumerate() {
            trail.push(format!("{}#{}", node.name, at));
            for (key, value) in &node.parameters {
                if key == "value" || key.starts_with("_computed_") {
                    out.insert(format!("{}/{}", trail.join("/"), key), format!("{value:?}"));
                }
            }
            walk(&node.children, trail, out);
            trail.pop();
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

fn value(nodes: &[OverseerNode], address: &str) -> OverseerValue {
    let node = overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("no {address}"));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("{address} has no value"))
}

/// The document as the API's caller is shown it, against the same worked out from nothing, and
/// held where its state says.
fn the_caller_sees_it_worked_out_afresh(service: &DocumentRoot, root: &std::path::Path, after: &str) {
    let text = std::fs::read_to_string(root.join("d.os")).expect("read");
    assert!(
        overseer::viewstate::worked_out_as("", "d.os", &app_api::version_of(&text)).is_some(),
        "after {after}, nothing says where the document as the caller sees it is held"
    );
    let shown = service.open_for("", "d.os").expect("open");
    let overlay = overseer::viewstate::overlay("", "d.os");
    let afresh = app_api::resolve_selective(format!("{text}\n"), overlay.keys().cloned().collect(), Some(overlay))
        .expect("worked out afresh");
    let (shown, afresh) = (values(&shown), values(&afresh));
    let differ: Vec<_> = afresh
        .iter()
        .filter(|(at, value)| shown.get(*at) != Some(*value))
        .map(|(at, value)| format!("{at}: shown {:?}, afresh {value}", shown.get(at)))
        .collect();
    assert!(differ.is_empty(), "after {after}, the caller is shown otherwise than afresh:\n{}", differ.join("\n"));
}

/// What someone who holds nothing of their own is shown: the file, worked out.
fn anyone_else_sees_the_file(service: &DocumentRoot, root: &std::path::Path, who: &str, after: &str) {
    let text = std::fs::read_to_string(root.join("d.os")).expect("read");
    let shown = values(&service.open_for(who, "d.os").expect("open"));
    let afresh = values(&app_api::resolve_selective(format!("{text}\n"), Vec::new(), None).expect("worked out afresh"));
    let differ: Vec<_> = afresh
        .iter()
        .filter(|(at, value)| shown.get(*at) != Some(*value))
        .map(|(at, value)| format!("{at}: shown {:?}, the file {value}", shown.get(at)))
        .collect();
    assert!(differ.is_empty(), "after {after}, {who} is shown the caller's board:\n{}", differ.join("\n"));
}

#[test]
fn a_cell_flagged_through_the_api_is_flagged_for_the_caller_alone() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("flag");
    let service = DocumentRoot::new(&root).expect("open the root");
    service.open_for("p", "d.os").expect("open");

    service.run_event("d.os", "game/Field/E5", "click").expect("the press");

    let theirs = service.open_for("", "d.os").expect("open");
    assert_eq!(value(&theirs, "game/Field/E5/flagged"), OverseerValue::Boolean(true), "the caller's flag");
    for who in ["p", "q"] {
        let shown = service.open_for(who, "d.os").expect("open");
        assert_eq!(value(&shown, "game/Field/E5/flagged"), OverseerValue::Boolean(false), "{who} sees the caller's flag");
    }
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), minesweeper(), "the press wrote the file");
}

#[test]
fn presses_through_the_api_start_from_what_the_caller_last_saw() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("presses");
    let service = DocumentRoot::new(&root).expect("open the root");

    let board = service.open("d.os").expect("open");
    let cells: Vec<String> = (1..=9)
        .flat_map(|row| ('A'..='I').map(move |column| format!("game/Field/{column}{row}")))
        .collect();
    let mine = |cell: &str| value(&board, &format!("{cell}/mine")) == OverseerValue::Boolean(true);
    let around = |cell: &str| value(&board, &format!("{cell}/around"));
    let numbered = cells.iter().find(|c| !mine(c) && around(c) != OverseerValue::Integer(0)).expect("a numbered cell");
    let empty = cells.iter().find(|c| !mine(c) && around(c) == OverseerValue::Integer(0)).expect("an empty cell");
    let flagged = cells.iter().rev().find(|c| !mine(c)).expect("a cell to flag");

    let moves: [(&str, &str); 5] = [
        (flagged, "click"),
        (flagged, "click"),
        (flagged, "click"),
        (numbered, "hold"),
        (empty, "hold"),
    ];
    for (address, event) in moves {
        service.run_event("d.os", address, event).unwrap_or_else(|e| panic!("{event} {address} was refused: {e:?}"));
        let after = format!("{event} on {address}");
        the_caller_sees_it_worked_out_afresh(&service, &root, &after);
        anyone_else_sees_the_file(&service, &root, "p", &after);
    }
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), minesweeper(), "playing wrote the file");
}
