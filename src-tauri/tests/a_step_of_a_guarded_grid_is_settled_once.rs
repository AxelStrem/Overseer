//! A press whose handler runs an `each` and then one more action - a game of life's step - is
//! settled between the two through the graph, and the settle at the end follows only what moved
//! after that, through the same graph. It used to drop that graph and work the whole document out
//! again, which was half of what a tick of a 20x20 board cost.
//!
//! And a bare name read from a cell of the grid, found further up, is found without searching
//! every other cell on the way: it says what it always said, and costs what an absolute path does.

use overseer::app_api;
use overseer::resolver::times_worked_out_whole;
use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use serde_json::json;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const LIFE: &str = r#"div (hidden=true) {
    div Cell {
        bool alive = false
        int x = $(column())
        int y = $(row())
        int left = $((x + W - 1) % W)
        int right = $((x + 1) % W)
        int up = $((y + H - 1) % H)
        int down = $((y + 1) % H)
        int n = $((Board.at(left, up)/alive ? 1 : 0) + (Board.at(x, up)/alive ? 1 : 0) + (Board.at(right, up)/alive ? 1 : 0)
                + (Board.at(left, y)/alive ? 1 : 0) + (Board.at(right, y)/alive ? 1 : 0)
                + (Board.at(left, down)/alive ? 1 : 0) + (Board.at(x, down)/alive ? 1 : 0) + (Board.at(right, down)/alive ? 1 : 0))
        bool next = $(alive ? n == 2 || n == 3 : n == 3)
        string note = $(greeting)
    }
}

div life (mutable="guarded", tick=1) {
    int W = 5
    int H = 5
    int generation = 0
    int population = $(Board.cells().filter(|c| c/alive).count())
    bool upright = $(Board.at(2, 1)/alive && Board.at(2, 2)/alive && Board.at(2, 3)/alive && !Board.at(1, 2)/alive)

    grid Board (entry=<Cell>, columns=5, rows=5) { }

    div about {
        div {
            string greeting = "hello"
        }
    }

    on tick {
        press (path="/life/step")
    }

    button blinker {
        on click {
            set (path="/life/Board[1,2]/alive") = true
            set (path="/life/Board[2,2]/alive") = true
            set (path="/life/Board[3,2]/alive") = true
        }
    }

    button step {
        on click {
            each (in=$(/life/Board.cells().filter(|c| c/alive != c/next)), as="c") {
                toggle (path="c/alive")
            }
            inc (path="/life/generation")
        }
    }
}
"#;

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_guarded_step_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), LIFE).expect("write");
    overseer::viewstate::forget_all();
    root
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

/// The document as this viewer is shown it, against the same worked out from the file's text with
/// their values applied.
fn agrees_with_working_it_out_afresh(service: &DocumentRoot, root: &std::path::Path, session: &str, after: &str) {
    let text = std::fs::read_to_string(root.join("d.os")).expect("read");
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

fn press(service: &DocumentRoot, session: &str, address: &str) {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, address).unwrap_or_else(|| panic!("no {address}"));
    service
        .command_for(session, Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .unwrap_or_else(|e| panic!("a press of {address} was refused: {e:?}"));
}

fn tick(service: &DocumentRoot, session: &str) {
    service
        .command_for(session, Some("d.os"), "run_overseer_tick", &json!({ "owners": ["life"] }))
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
fn a_blinker_steps_and_ticks_without_the_board_being_worked_out_whole() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("blinker");
    let service = DocumentRoot::new(&root).expect("open the root");

    service.open_for("p", "d.os").expect("open");
    press(&service, "p", "life/blinker");
    // The first press of a viewer can find nothing held for them; every one after starts from
    // what they last saw.
    press(&service, "p", "life/step");
    let shown = service.open_for("p", "d.os").expect("open");
    assert_eq!(value(&shown, "life/upright"), OverseerValue::Boolean(true), "a step stands the blinker up");
    agrees_with_working_it_out_afresh(&service, &root, "p", "the first step");

    let whole = times_worked_out_whole();
    for round in 0..4 {
        if round % 2 == 0 {
            press(&service, "p", "life/step");
        } else {
            tick(&service, "p");
        }
        assert_eq!(times_worked_out_whole(), whole, "step {} worked the document out whole", round + 2);
        let shown = service.open_for("p", "d.os").expect("open");
        assert_eq!(value(&shown, "life/generation"), OverseerValue::Integer(round + 2));
        assert_eq!(value(&shown, "life/population"), OverseerValue::Integer(3));
        assert_eq!(value(&shown, "life/upright"), OverseerValue::Boolean(round % 2 == 1), "after step {}", round + 2);
        agrees_with_working_it_out_afresh(&service, &root, "p", &format!("step {}", round + 2));
    }
    assert_eq!(std::fs::read_to_string(root.join("d.os")).expect("read"), LIFE, "the file is the viewer's to leave alone");
}

#[test]
fn a_bare_name_read_from_a_cell_finds_what_is_above_it_and_what_is_deep_beside_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("names");
    let service = DocumentRoot::new(&root).expect("open the root");

    let shown = service.open_for("p", "d.os").expect("open");
    for (column, row) in [(0, 0), (4, 0), (2, 3), (4, 4)] {
        let cell = format!("life/Board/{}{}", (b'A' + column as u8) as char, row + 1);
        assert_eq!(value(&shown, &format!("{cell}/left")), OverseerValue::Integer((column + 4) % 5), "{cell}");
        assert_eq!(value(&shown, &format!("{cell}/down")), OverseerValue::Integer((row + 1) % 5), "{cell}");
        assert_eq!(
            value(&shown, &format!("{cell}/note")),
            OverseerValue::String("hello".into()),
            "{cell} finds a name nested beside the grid"
        );
    }
}
