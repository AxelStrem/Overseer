//! A document's `on tick` runs on each beat of the page's clock, for whoever has it open.
//!
//! A tick is held to what `on load` is held to: it may change only fields marked
//! `mutable="guarded"`, kept against the viewer's session, so each page steps its own state and
//! the file is never written. Anything else is refused whole.

use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;
use serde_json::json;

const COUNTER: &str = r#"tab clock (mutable=true, tick=$(running ? "250ms" : 0)) {
    bool running (mutable="guarded") = true
    int beats (mutable="guarded") = 0

    on tick {
        inc (path="/clock/beats")
    }
}
"#;

const WRITES_THE_FILE: &str = r#"tab clock (mutable=true, tick=1) {
    int beats (mutable="guarded") = 0
    int written = 0

    on tick {
        inc (path="/clock/beats")
        inc (path="/clock/written")
    }
}
"#;

const TWO_CLOCKS: &str = r#"tab fast (mutable=true, tick=1) {
    int beats (mutable="guarded") = 0

    on tick {
        inc (path="/fast/beats")
    }
}

tab slow (mutable=true, tick=5) {
    int beats (mutable="guarded") = 0

    on tick {
        inc (path="/slow/beats")
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str, document: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_tick_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), document).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn tick(service: &DocumentRoot, session: &str, owners: &[&str]) -> Result<serde_json::Value, String> {
    service
        .command_for(session, Some("d.os"), "run_overseer_tick", &json!({ "owners": owners }))
        .map_err(|e| format!("{:?}", e))
}

fn held(session: &str, address: &str) -> Option<i64> {
    match overseer::viewstate::overlay(session, "d.os").get(address) {
        Some(OverseerValue::Integer(n)) => Some(*n),
        Some(other) => panic!("{} is held as {:?}", address, other),
        None => None,
    }
}

#[test]
fn each_tick_steps_the_viewers_state_and_the_file_is_left_alone() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("counter", COUNTER);
    let service = DocumentRoot::new(&root).expect("open the root");

    for _ in 0..3 {
        tick(&service, "alice", &["clock"]).expect("a tick was refused");
    }
    tick(&service, "bob", &["clock"]).expect("a tick was refused");

    assert_eq!(held("alice", "clock/beats"), Some(3));
    assert_eq!(held("bob", "clock/beats"), Some(1), "one viewer's clock stepped another's state");
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), COUNTER, "a tick wrote the file");
    let (beside, named) = overseer::undo::beside(&root.join("d.os")).expect("an undo folder");
    assert_eq!(overseer::undo::depth(&beside, &named), 0, "a tick left a step to take back");
}

#[test]
fn a_tick_that_writes_the_document_is_refused_whole() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("refused", WRITES_THE_FILE);
    let service = DocumentRoot::new(&root).expect("open the root");

    let refused = tick(&service, "alice", &["clock"]).expect_err("a tick writing the file was run");
    assert!(refused.contains("guarded"), "the refusal does not say why: {}", refused);
    assert_eq!(
        std::fs::read_to_string(root.join("d.os")).unwrap(),
        WRITES_THE_FILE,
        "a refused tick wrote the file"
    );
    assert_eq!(held("alice", "clock/beats"), None, "a refused tick still stepped the viewer's state");
}

#[test]
fn a_tick_runs_only_the_clocks_it_names() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("two", TWO_CLOCKS);
    let service = DocumentRoot::new(&root).expect("open the root");

    tick(&service, "alice", &["fast"]).expect("a tick was refused");
    tick(&service, "alice", &["fast"]).expect("a tick was refused");
    tick(&service, "alice", &["slow"]).expect("a tick was refused");

    assert_eq!(held("alice", "fast/beats"), Some(2));
    assert_eq!(held("alice", "slow/beats"), Some(1));
}

#[test]
fn the_interval_can_be_a_formula() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = overseer::app_api::load_document(COUNTER.to_string()).expect("the document parses");
    let clock = nodes.iter().find(|n| n.name == "clock").expect("the clock tab");
    assert_eq!(
        clock.parameters.get("_computed_tick"),
        Some(&OverseerValue::String("250ms".into())),
        "tick= was not worked out from running"
    );
}
