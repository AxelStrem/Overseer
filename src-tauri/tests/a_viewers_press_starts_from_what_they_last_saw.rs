//! A press, a tick or an open on a document holding something for the viewer starts from the
//! document as that viewer last saw it, and follows what moved through the graph - and says the
//! same as working the document out afresh with the viewer's values.
//!
//! It used to work the whole document out twice per press: once to apply the viewer's values,
//! because nothing was held for them, and again to settle, because a value of theirs was not one
//! the graph was asked about. That was always right, which is what this holds the quick way to.

use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use serde_json::json;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const COUNTER: &str = r#"tab clock (mutable=true, tick=1) {
    int beats (mutable="guarded") = 0
    int step = 1
    int total = $(beats * step)
    string parity = $(beats % 2 == 0 ? "even" : "odd")

    on tick {
        inc (path="/clock/beats")
    }

    button faster {
        on click {
            inc (path="/clock/step")
        }
    }
}
"#;

fn a_root(tag: &str, document: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_viewers_press_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), document).expect("write");
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

/// The document as this viewer is shown it, held for them and copied, against the same worked out
/// from nothing: the file's text with their values applied, under a text nothing is held for.
fn agrees_with_working_it_out_afresh(service: &DocumentRoot, root: &std::path::Path, session: &str, after: &str) {
    let text = std::fs::read_to_string(root.join("d.os")).expect("read");
    assert!(
        overseer::viewstate::worked_out_as(session, "d.os", &app_api::version_of(&text)).is_some(),
        "after {after}, nothing says where the document as {session} sees it is held"
    );
    let shown = service.open_for(session, "d.os").expect("open");
    let overlay = overseer::viewstate::overlay(session, "d.os");
    let afresh = app_api::resolve_selective(format!("{text}\n"), overlay.keys().cloned().collect(), Some(overlay))
        .expect("worked out afresh");
    let (shown, afresh) = (values(&shown), values(&afresh));
    let differ: Vec<_> = afresh
        .iter()
        .filter(|(at, value)| shown.get(*at) != Some(*value))
        .map(|(at, value)| format!("{at}: shown {:?}, afresh {value}", shown.get(at)))
        .chain(shown.keys().filter(|at| !afresh.contains_key(*at)).map(|at| format!("{at}: only shown")))
        .collect();
    assert!(differ.is_empty(), "after {after}, {session} is shown otherwise than afresh:\n{}", differ.join("\n"));
}

fn send(service: &DocumentRoot, session: &str, address: &str, event: &str) {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, address).unwrap_or_else(|| panic!("no {address}"));
    service
        .command_for(session, Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": event }))
        .unwrap_or_else(|e| panic!("{event} {address} was refused: {e:?}"));
}

fn tick(service: &DocumentRoot, session: &str) {
    service
        .command_for(session, Some("d.os"), "run_overseer_tick", &json!({ "owners": ["clock"] }))
        .unwrap_or_else(|e| panic!("a tick was refused: {e:?}"));
}

fn value(nodes: &[OverseerNode], address: &str) -> OverseerValue {
    let node = overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("no {address}"));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("{address} has no value"))
}

#[test]
fn a_game_played_by_two_viewers_is_shown_to_each_as_worked_out_afresh() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("game", &minesweeper());
    let service = DocumentRoot::new(&root).expect("open the root");

    for session in ["p", "q"] {
        service
            .command_for(session, Some("d.os"), "run_overseer_on_load", &json!({}))
            .expect("on load was refused");
        agrees_with_working_it_out_afresh(&service, &root, session, "on load");
    }

    let board = service.open_for("p", "d.os").expect("open");
    let cells: Vec<String> = (1..=9)
        .flat_map(|row| ('A'..='I').map(move |column| format!("game/Field/{column}{row}")))
        .collect();
    let mine = |cell: &str| value(&board, &format!("{cell}/mine")) == OverseerValue::Boolean(true);
    let around = |cell: &str| value(&board, &format!("{cell}/around"));
    let empty = cells.iter().find(|c| !mine(c) && around(c) == OverseerValue::Integer(0)).expect("an empty cell");
    let numbered = cells.iter().find(|c| !mine(c) && around(c) != OverseerValue::Integer(0)).expect("a numbered cell");
    let flagged = cells.iter().rev().find(|c| !mine(c)).expect("a cell to flag");
    let a_mine = cells.iter().find(|c| mine(c)).expect("a mine");

    let moves: [(&str, &str, &str); 7] = [
        ("p", flagged, "click"),
        ("q", numbered, "hold"),
        ("p", empty, "hold"),
        ("p", numbered, "hold"),
        ("p", flagged, "click"),
        ("p", a_mine, "hold"),
        ("p", "game/new_game", "click"),
    ];
    for (session, address, event) in moves {
        send(&service, session, address, event);
        let after = format!("{session}'s {event} on {address}");
        agrees_with_working_it_out_afresh(&service, &root, session, &after);
    }
    agrees_with_working_it_out_afresh(&service, &root, "q", "all of it");
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), minesweeper(), "playing wrote the file");
}

#[test]
fn ticks_are_shown_as_worked_out_afresh_and_a_write_to_the_file_reaches_them() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("ticks", COUNTER);
    let service = DocumentRoot::new(&root).expect("open the root");

    for beat in 1..=3 {
        tick(&service, "p");
        agrees_with_working_it_out_afresh(&service, &root, "p", &format!("tick {beat}"));
    }
    tick(&service, "q");
    agrees_with_working_it_out_afresh(&service, &root, "q", "q's tick");

    send(&service, "q", "clock/faster", "click");
    assert!(std::fs::read_to_string(root.join("d.os")).unwrap().contains("step = 2"), "the press wrote nothing");
    agrees_with_working_it_out_afresh(&service, &root, "q", "q's press");

    tick(&service, "p");
    agrees_with_working_it_out_afresh(&service, &root, "p", "p's tick after the file moved");
    let shown = service.open_for("p", "d.os").expect("open");
    assert_eq!(value(&shown, "clock/total"), OverseerValue::Integer(8), "four beats of two");
}
