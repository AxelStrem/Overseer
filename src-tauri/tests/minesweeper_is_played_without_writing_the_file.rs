//! `examples/games/minesweeper.os` is a game played in the open document: every cell is the
//! viewer's, so opening it, digging, flagging and starting again never write the file.

use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn the_game() -> String {
    let at = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/games/minesweeper.os");
    std::fs::read_to_string(at).expect("the example")
}

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_minesweeper_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), the_game()).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn send(service: &DocumentRoot, address: &str, event: &str) {
    let nodes = service.open("d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, address).unwrap_or_else(|| panic!("no {}", address));
    service
        .command_for("p", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": event }))
        .unwrap_or_else(|e| panic!("{} {} was refused: {:?}", event, address, e));
}

fn value(nodes: &[OverseerNode], address: &str) -> OverseerValue {
    let node = overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("no {address}"));
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("{address} has no value"))
}

fn cells() -> Vec<String> {
    (0..9).flat_map(|row| (0..9).map(move |column| format!("game/Field/{}{}", (b'A' + column) as char, row + 1))).collect()
}

fn opened() -> Vec<String> {
    overseer::viewstate::overlay("p", "d.os")
        .into_iter()
        .filter(|(address, v)| address.ends_with("/opened") && matches!(v, OverseerValue::Boolean(true)))
        .map(|(address, _)| address.trim_end_matches("/opened").to_string())
        .collect()
}

#[test]
fn there_are_ten_mines_and_holding_an_empty_cell_opens_its_whole_region() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("flood");
    let service = DocumentRoot::new(&root).expect("open the root");
    let nodes = service.open("d.os").expect("open");
    let mine = |cell: &str| value(&nodes, &format!("{cell}/mine")) == OverseerValue::Boolean(true);
    let around = |cell: &str| match value(&nodes, &format!("{cell}/around")) {
        OverseerValue::Integer(n) => n,
        other => panic!("{cell} around is {other:?}"),
    };
    assert_eq!(cells().iter().filter(|c| mine(c)).count(), 10);

    let empty = cells().into_iter().find(|c| !mine(c) && around(c) == 0).expect("no empty cell");
    send(&service, &empty, "hold");
    let open = opened();
    assert!(open.len() > 1, "a hold on an empty cell opened only {:?}", open);
    assert!(open.iter().all(|c| !mine(c)), "the flood opened a mine: {:?}", open);
    // Every opened cell with nothing around it opened all of its neighbours.
    let place = |c: &str| {
        let name = c.rsplit('/').next().unwrap().as_bytes();
        ((name[0] - b'A') as i32, (name[1] - b'1') as i32)
    };
    for c in open.iter().filter(|c| around(c) == 0) {
        let (x, y) = place(c);
        for n in cells().iter().filter(|n| {
            let (nx, ny) = place(n);
            (nx - x).abs() <= 1 && (ny - y).abs() <= 1
        }) {
            assert!(open.contains(n), "{c} is empty but {n} beside it stayed closed");
        }
    }

    send(&service, "game/new_game", "click");
    assert!(opened().is_empty(), "a new game left cells open: {:?}", opened());
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), the_game(), "playing wrote the file");
}

#[test]
fn opening_the_game_deals_a_board_for_the_viewer_and_writes_nothing() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("load");
    let service = DocumentRoot::new(&root).expect("open the root");
    service
        .command_for("p", Some("d.os"), "run_overseer_on_load", &json!({}))
        .expect("on load was refused");
    assert!(
        overseer::viewstate::overlay("p", "d.os").contains_key("game/seed"),
        "on load dealt no board"
    );
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), the_game(), "opening wrote the file");
    let (beside, named) = overseer::undo::beside(&root.join("d.os")).expect("an undo folder");
    assert_eq!(overseer::undo::depth(&beside, &named), 0, "opening left a step to take back");
}
