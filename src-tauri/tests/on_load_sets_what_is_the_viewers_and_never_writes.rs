//! A document's `on load` runs when the page opens it, for whoever opened it.
//!
//! Opening a document must never write it, so what `on load` may change is only what is the
//! viewer's - fields marked `mutable="guarded"`, kept against their session. Anything else is
//! refused whole, and the file is left byte for byte as it was, with nothing to take back.

use overseer::server::DocumentRoot;
use serde_json::json;

const GUARDED: &str = r#"tab board (mutable=true) {
    timestamp started (mutable="guarded") = ""
    int games = 0

    on load {
        set_now_ts (path="/board/started")
    }
}
"#;

const WRITES_THE_FILE: &str = r#"tab board (mutable=true) {
    timestamp started (mutable="guarded") = ""
    int games = 0

    on load {
        set_now_ts (path="/board/started")
        inc (path="/board/games", by=1)
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str, document: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_onload_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), document).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn open(service: &DocumentRoot, session: &str) -> Result<serde_json::Value, String> {
    service
        .command_for(session, Some("d.os"), "run_overseer_on_load", &json!({}))
        .map_err(|e| format!("{:?}", e))
}

fn started(session: &str) -> Option<overseer::types::OverseerValue> {
    overseer::viewstate::overlay(session, "d.os").get("board/started").cloned()
}

#[test]
fn a_guarded_field_is_set_for_the_viewer_and_the_file_is_left_alone() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("guarded", GUARDED);
    let service = DocumentRoot::new(&root).expect("open the root");

    open(&service, "alice").expect("on load was refused");

    let set = started("alice").expect("on load set nothing for the viewer");
    assert!(
        !matches!(set, overseer::types::OverseerValue::String(ref s) if s.is_empty()),
        "on load left the field as authored: {:?}",
        set
    );
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), GUARDED, "opening wrote the file");
    let (beside, named) = overseer::undo::beside(&root.join("d.os")).expect("an undo folder");
    assert_eq!(overseer::undo::depth(&beside, &named), 0, "opening left a step to take back");
    assert!(started("bob").is_none(), "another viewer was given what alice opened with");
}

#[test]
fn opening_again_sets_it_afresh() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("again", GUARDED);
    let service = DocumentRoot::new(&root).expect("open the root");

    open(&service, "alice").expect("on load was refused");
    let first = started("alice").expect("set the first time");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    open(&service, "alice").expect("on load was refused the second time");
    let second = started("alice").expect("set the second time");
    assert_ne!(first, second, "a second opening kept the first opening's moment");
}

#[test]
fn a_load_that_writes_the_document_is_refused_whole() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("refused", WRITES_THE_FILE);
    let service = DocumentRoot::new(&root).expect("open the root");

    let refused = open(&service, "alice").expect_err("a load writing the file was run");
    assert!(refused.contains("guarded"), "the refusal does not say why: {}", refused);
    assert_eq!(
        std::fs::read_to_string(root.join("d.os")).unwrap(),
        WRITES_THE_FILE,
        "a refused load wrote the file"
    );
    assert!(started("alice").is_none(), "a refused load still set the viewer's field");
}

#[test]
fn a_document_without_on_load_is_answered_unchanged() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let plain = "tab board (mutable=true) {\n    int games = 0\n}\n";
    let root = a_root("plain", plain);
    let service = DocumentRoot::new(&root).expect("open the root");

    open(&service, "alice").expect("a document with nothing to run was refused");
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), plain);
    assert!(overseer::viewstate::overlay("alice", "d.os").is_empty());
}
