//! Undo takes back a write to a document in a folder, whichever way the write came.
//!
//! The server named a document by its path under the root and kept its history at the root; a
//! page's press, written through `app_api`, knows only the file and kept it beside the file. For a
//! document at the root the two are one place. For one in a folder - every project - they were
//! two: the press recorded its step where Undo never looked, and Undo on a project said there was
//! nothing to take back. Found when a task dropped by mistake could not be put back.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    int count = 0

    button bump (label="bump") {
        on click {
            inc (path="/t/count", by=1)
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_undo_folder_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("projects")).unwrap();
    std::fs::write(root.join("projects").join("p.os"), DOCUMENT).unwrap();
    overseer::app_api::forget_baseline();
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("projects").join("p.os")).unwrap()
}

fn undo(service: &DocumentRoot) -> serde_json::Value {
    service
        .command(Some("projects/p.os"), "undo_overseer_file", &json!({ "path": "projects/p.os" }))
        .expect("there was nothing to take back")
}

#[test]
fn a_press_from_the_page_can_be_taken_back() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("page");
    let service = DocumentRoot::new(&root).unwrap();
    service
        .command_for("alice", Some("projects/p.os"), "parse_overseer_content", &json!({ "content": DOCUMENT }))
        .expect("open");
    service
        .command_for(
            "alice",
            Some("projects/p.os"),
            "run_overseer_event",
            &json!({ "path": "projects/p.os", "node_path": ["t", "bump"], "event_name": "click" }),
        )
        .expect("the press");
    assert!(on_disk(&root).contains("int count = 1"));

    let answer = undo(&service);
    assert_eq!(on_disk(&root), DOCUMENT, "the press was not taken back");
    assert_eq!(answer["steps_left"], 0);
}

#[test]
fn a_write_from_the_bot_can_be_taken_back_too() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("bot");
    let service = DocumentRoot::new(&root).unwrap();
    service.run_event("projects/p.os", "t/bump", "click").expect("the bot's press");
    assert!(on_disk(&root).contains("int count = 1"));

    undo(&service);
    assert_eq!(on_disk(&root), DOCUMENT, "the bot's write was not taken back");
}

#[test]
fn both_kinds_of_write_are_one_history() {
    // A press from the page, then one from the bot: two steps, taken back newest first.
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("both");
    let service = DocumentRoot::new(&root).unwrap();
    service
        .command_for(
            "alice",
            Some("projects/p.os"),
            "run_overseer_event",
            &json!({ "path": "projects/p.os", "node_path": ["t", "bump"], "event_name": "click" }),
        )
        .expect("the page's press");
    service.run_event("projects/p.os", "t/bump", "click").expect("the bot's press");
    assert!(on_disk(&root).contains("int count = 2"));

    assert_eq!(undo(&service)["steps_left"], 1);
    assert!(on_disk(&root).contains("int count = 1"), "the newest step was not the one taken back");
    assert_eq!(undo(&service)["steps_left"], 0);
    assert_eq!(on_disk(&root), DOCUMENT);
}
