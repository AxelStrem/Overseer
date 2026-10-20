//! A program can save its own files as Overseer text under an extension of its own - Brocessor
//! its `.brot` patches - and name that extension in `extensions.txt` beside the settings, as it
//! names its library folder in `libraries.txt`. Such a file is then a document like any `.os`
//! one: listed, opened and written back under its own name. An extension nobody named is still
//! refused.

use overseer::server::{DocumentRoot, RequestError};
use overseer::types::*;
use std::path::PathBuf;
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const PATCH: &str = "tab patch (mutable=true) {\r\n    string name = \"Lead\"\r\n}\r\n";

fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "overseer_document_extensions_{}_{}",
        std::process::id(),
        test
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        std::fs::write(root.join(name), text).unwrap();
    }
    root
}

fn name_in(service: &DocumentRoot, document: &str) -> Option<OverseerValue> {
    let found = service.read_at(document, "patch/name").ok()?;
    found.node.parameters.get("value").cloned()
}

#[test]
fn a_file_under_a_listed_extension_is_a_document() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder(
        "listed",
        &[("p.brot", PATCH), ("notes.txt", PATCH), ("extensions.txt", "# Brocessor\n\n.brot\n")],
    );
    let service = DocumentRoot::new(&root).unwrap();

    assert_eq!(service.list(), vec!["p.brot".to_string()]);
    assert_eq!(name_in(&service, "p.brot"), Some(OverseerValue::String("Lead".into())));

    service
        .set_at("p.brot", "patch/name", OverseerValue::String("Pad".into()))
        .expect("the write");
    let written = std::fs::read_to_string(root.join("p.brot")).unwrap();
    assert_eq!(written, PATCH.replace("Lead", "Pad"));
    assert!(!root.join("p.os").exists());

    assert!(matches!(service.resolve("notes.txt"), Err(RequestError::Rejected(_))));
}

#[test]
fn an_extension_nobody_named_is_refused() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("unlisted", &[("p.brot", PATCH), ("d.os", PATCH)]);
    let service = DocumentRoot::new(&root).unwrap();

    assert_eq!(service.list(), vec!["d.os".to_string()]);
    assert!(matches!(service.resolve("p.brot"), Err(RequestError::Rejected(_))));
}

#[test]
fn the_settings_say_which_extensions_a_document_may_have() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("settings", &[("extensions.txt", "brot\nBROT\nos\n")]);
    let service = DocumentRoot::new(&root).unwrap();

    let read = service.command(None, "settings_read", &serde_json::json!({})).expect("the settings");
    assert_eq!(read["extensions"], serde_json::json!(["os", "brot"]));
}
