//! The shipped theme keeps each mode's colours under `light` and `dark` and picks one by `mode`,
//! so a document that changes `dark/background` changes the dark page alone, and the light page
//! keeps the shipped colour.

use overseer::environment;
use overseer::server::DocumentRoot;
use overseer::types::*;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, files: &[(&str, &str)]) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_themesplit_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        std::fs::write(root.join(name), text).unwrap();
    }
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn text_at(nodes: &[OverseerNode], address: &str) -> Option<String> {
    let node = overseer::addressing::find(nodes, address)?;
    match node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
    {
        Some(OverseerValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

const DOCUMENT: &str = "context theme {\r\n    - dark {\r\n        - background = \"#000000\"\r\n    }\r\n}\r\n\r\ntab t (label=\"T\", mutable=true) {\r\n    string background = $(theme/background)\r\n    string accent = $(theme/accent)\r\n\r\n    div side {\r\n        context theme {\r\n            - light {\r\n                - accent = \"#ff0000\"\r\n            }\r\n        }\r\n        string background = $(theme/background)\r\n        string accent = $(theme/accent)\r\n    }\r\n}\r\n";

#[test]
fn a_document_changes_the_dark_background_and_the_light_one_stays_shipped() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("background", &[("p.os", DOCUMENT)]);

    let light = environment::seen_as(Some("light"), || documents.open("p.os")).expect("open light");
    assert_eq!(text_at(&light, "t/background").as_deref(), Some("#f8fafc"));
    assert_eq!(text_at(&light, "t/accent").as_deref(), Some("#2563eb"));

    let dark = environment::seen_as(Some("dark"), || documents.open("p.os")).expect("open dark");
    assert_eq!(text_at(&dark, "t/background").as_deref(), Some("#000000"));
    assert_eq!(text_at(&dark, "t/accent").as_deref(), Some("#3b82f6"));
}

#[test]
fn a_part_changes_one_light_colour_and_keeps_the_rest_of_what_is_above() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("part", &[("p.os", DOCUMENT)]);

    let light = environment::seen_as(Some("light"), || documents.open("p.os")).expect("open light");
    assert_eq!(text_at(&light, "t/side/accent").as_deref(), Some("#ff0000"));
    assert_eq!(text_at(&light, "t/side/background").as_deref(), Some("#f8fafc"));

    let dark = environment::seen_as(Some("dark"), || documents.open("p.os")).expect("open dark");
    assert_eq!(text_at(&dark, "t/side/accent").as_deref(), Some("#3b82f6"));
    assert_eq!(text_at(&dark, "t/side/background").as_deref(), Some("#000000"));
}
