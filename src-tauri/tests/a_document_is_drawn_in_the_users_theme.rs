//! Every document is worked out inside the user's settings: a theme and a mode, kept in
//! `settings.os` beside the documents, seen at the top of each document as contexts.
//!
//! A document's own `context theme` changes some keys and keeps the rest; `mode = "system"` is the
//! scheme the page reports; a mounted document sees the theme of the place it is mounted at; and
//! the drawer's choice of theme and mode reaches the settings file, keeping what the user changed.

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
    let root = std::env::temp_dir().join(format!("overseer_theme_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("themes")).unwrap();
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

const READER: &str = "tab t (label=\"T\", mutable=true) {\r\n    string accent = $(theme/accent)\r\n    string surface = $(theme/surface)\r\n    string seen = $(mode)\r\n    string other = \"\"\r\n}\r\n";

const PAPER: &str = "context theme {\r\n    - accent = \"brown\"\r\n    - surface = $(mode == \"dark\" ? \"ink\" : \"paper\")\r\n}\r\n";

#[test]
fn with_no_settings_a_document_is_drawn_in_the_shipped_theme_as_the_page_is() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("shipped", &[("p.os", READER)]);

    let light = documents.open("p.os").expect("open");
    assert_eq!(text_at(&light, "t/accent").as_deref(), Some("#2563eb"));
    assert_eq!(text_at(&light, "t/surface").as_deref(), Some("#ffffff"));
    assert_eq!(text_at(&light, "t/seen").as_deref(), Some("light"), "an unreported scheme is light");

    let dark = environment::seen_as(Some("dark"), || documents.open("p.os")).expect("open dark");
    assert_eq!(text_at(&dark, "t/surface").as_deref(), Some("#1e293b"));
    assert_eq!(text_at(&dark, "t/seen").as_deref(), Some("dark"));

    // Held worked out under each scheme apart: the light page is not handed the dark copy.
    let light_again = documents.open("p.os").expect("reopen");
    assert_eq!(text_at(&light_again, "t/surface").as_deref(), Some("#ffffff"));
}

#[test]
fn a_document_changes_one_key_of_the_theme_and_keeps_the_rest() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let own = format!("context theme {{\r\n    - accent = \"red\"\r\n}}\r\n{}", READER);
    let (_sandbox, documents) = sandbox("own", &[("p.os", &own)]);
    let nodes = documents.open("p.os").expect("open");
    assert_eq!(text_at(&nodes, "t/accent").as_deref(), Some("red"));
    assert_eq!(text_at(&nodes, "t/surface").as_deref(), Some("#ffffff"));
}

#[test]
fn a_part_of_a_document_changes_how_one_kind_of_element_looks_and_keeps_the_rest() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = "tab t (label=\"T\") {
    string outside = $(theme/checkbox/style)
    div side {
        context theme {
            - checkbox {
                - style = \"switch\"
            }
        }
        string inside = $(theme/checkbox/style)
        string accent = $(theme/accent)
    }
}
";
    let (_sandbox, documents) = sandbox("kind", &[("p.os", doc)]);
    let nodes = documents.open("p.os").expect("open");
    assert_eq!(text_at(&nodes, "t/outside").as_deref(), Some("box"), "the shipped theme draws boxes");
    assert_eq!(text_at(&nodes, "t/side/inside").as_deref(), Some("switch"));
    assert_eq!(text_at(&nodes, "t/side/accent").as_deref(), Some("#2563eb"), "the other keys are kept");
}

#[test]
fn a_mode_the_settings_choose_is_not_the_systems() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let settings = "context theme (base=<themes/default.os/theme>) { }\r\ncontext mode = \"dark\"\r\n";
    let (_sandbox, documents) =
        sandbox("chosen", &[("p.os", READER), (environment::FILE, settings)]);
    let nodes = environment::seen_as(Some("light"), || documents.open("p.os")).expect("open");
    assert_eq!(text_at(&nodes, "t/surface").as_deref(), Some("#1e293b"));
}

#[test]
fn a_change_to_the_settings_reaches_a_document_held_worked_out() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("change", &[("p.os", READER)]);
    let before = documents.open("p.os").expect("open");
    assert_eq!(text_at(&before, "t/surface").as_deref(), Some("#ffffff"));

    std::fs::write(
        sandbox.0.join(environment::FILE),
        "context theme (base=<themes/default.os/theme>) {\r\n    - surface = \"cream\"\r\n}\r\ncontext mode = \"system\"\r\n",
    )
    .unwrap();
    let after = documents.open("p.os").expect("reopen");
    assert_eq!(text_at(&after, "t/surface").as_deref(), Some("cream"));
}

#[test]
fn a_mounted_document_is_drawn_in_the_theme_of_where_it_is_mounted() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let host = "context theme {\r\n    - accent = \"host\"\r\n}\r\ntab t (label=\"T\") {\r\n    mount M (lazy=false, mutable=false, source=\"side.os/s\") { }\r\n}\r\n";
    let side = "tab s (label=\"S\") {\r\n    string seen = $(theme/accent)\r\n    string surface = $(theme/surface)\r\n}\r\n";
    let (_sandbox, documents) = sandbox("mount", &[("host.os", host), ("side.os", side)]);
    let nodes = documents.open("host.os").expect("open");
    let seen = overseer::addressing::find(&nodes, "t/M/s/seen")
        .or_else(|| overseer::addressing::find(&nodes, "t/M/seen"))
        .expect("the mounted field");
    let value = |node: &OverseerNode| match node.parameters.get("_computed_value") {
        Some(OverseerValue::String(s)) => s.clone(),
        other => format!("{:?}", other),
    };
    assert_eq!(value(seen), "host");
    let surface = overseer::addressing::find(&nodes, "t/M/s/surface")
        .or_else(|| overseer::addressing::find(&nodes, "t/M/surface"))
        .expect("the mounted field");
    assert_eq!(value(surface), "#ffffff");
}

#[test]
fn a_field_named_like_the_environments_context_is_the_documents_own() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let doc = "tab t (label=\"T\") {\r\n    div rule {\r\n        string mode = \"interval\"\r\n        string said = $(mode)\r\n    }\r\n}\r\n";
    let (_sandbox, documents) = sandbox("clash", &[("p.os", doc)]);
    let nodes = documents.open("p.os").expect("open");
    let field = overseer::addressing::find(&nodes, "t/rule/mode").expect("the field");
    assert!(
        field.parameters.get("_inherit_error").is_none(),
        "a document that never declared `mode` was told it is taken"
    );
    assert_eq!(text_at(&nodes, "t/rule/said").as_deref(), Some("interval"));
}

#[test]
fn a_write_elsewhere_never_writes_the_environment_into_the_document() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("write", &[("p.os", READER)]);
    documents.open("p.os").expect("open");
    documents
        .set_at("p.os", "t/other", OverseerValue::String("x".to_string()))
        .expect("the write");
    let written = std::fs::read_to_string(sandbox.0.join("p.os")).unwrap();
    assert_eq!(written, READER.replace("string other = \"\"", "string other = \"x\""));
}

#[test]
fn choosing_a_mode_writes_it_to_the_settings() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("mode", &[("p.os", READER)]);
    let chosen = documents
        .command(None, "settings_choose", &serde_json::json!({ "mode": "dark" }))
        .expect("the choice");
    assert_eq!(chosen["mode"], "dark");
    let written = std::fs::read_to_string(sandbox.0.join(environment::FILE)).unwrap();
    assert_eq!(written, environment::DEFAULT.replace("context mode = \"system\"", "context mode = \"dark\""));
    let nodes = documents.open("p.os").expect("open");
    assert_eq!(text_at(&nodes, "t/surface").as_deref(), Some("#1e293b"));
}

#[test]
fn choosing_a_theme_keeps_what_the_user_changed_of_the_last_one() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let settings = "context theme (base=<themes/default.os/theme>) {\r\n    - accent = \"pink\"\r\n}\r\ncontext mode = \"system\"\r\n";
    let (sandbox, documents) = sandbox(
        "pick",
        &[("p.os", READER), (environment::FILE, settings), ("themes/paper.os", PAPER)],
    );
    let read = documents
        .command(None, "settings_read", &serde_json::json!({}))
        .expect("the settings");
    let themes: Vec<String> = serde_json::from_value(read["themes"].clone()).unwrap();
    assert!(themes.contains(&"default".to_string()) && themes.contains(&"paper".to_string()), "{:?}", themes);
    assert_eq!(read["theme"], "default");

    let chosen = documents
        .command(None, "settings_choose", &serde_json::json!({ "theme": "paper" }))
        .expect("the choice");
    assert_eq!(chosen["theme"], "paper");
    let written = std::fs::read_to_string(sandbox.0.join(environment::FILE)).unwrap();
    assert_eq!(written, settings.replace("themes/default.os", "themes/paper.os"));

    let nodes = documents.open("p.os").expect("open");
    assert_eq!(text_at(&nodes, "t/accent").as_deref(), Some("pink"));
    assert_eq!(text_at(&nodes, "t/surface").as_deref(), Some("paper"));
}

#[test]
fn a_theme_that_is_not_there_is_refused() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("missing", &[("p.os", READER)]);
    assert!(documents
        .command(None, "settings_choose", &serde_json::json!({ "theme": "../p" }))
        .is_err());
    assert!(!sandbox.0.join(environment::FILE).exists());
}
