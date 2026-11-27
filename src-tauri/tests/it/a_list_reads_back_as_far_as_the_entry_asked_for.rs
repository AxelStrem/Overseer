//! Asking a list for more `until` a key draws the entry with that key for that viewer, whatever
//! the window says, and nothing else: a chip pressed for a closed item weeks back opens the
//! history on it, with only the part that holds it read - not every window between, and not the
//! rest of the history when no part holds the key. It stays drawn through the catch-ups after.

use crate::common::serialised;
use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::{json, Value};
use std::path::PathBuf;

const DOCUMENT: &str = r#"tab log (label="Log", mutable=true) {
    div (hidden=true) {
        div Done (layout="horizontal") {
            string handle = ""
            timestamp done_at = "2026-11-02T10:00:00Z"
        }
    }

    list History (entry=<Done>, key="handle", window=1, parts=$(|x| week(x/done_at)),
                  sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) { }

    list Days (key="day", window=2) {
        - { - day = "mon" }
        - { - day = "tue" }
        - { - day = "wed" }
        - { - day = "thu" }
        - { - day = "fri" }
    }

    string note = ""
}
"#;

/// One entry a week, `w40` to `w45`, each in its week's part.
const WEEKS: [(&str, &str); 6] = [
    ("2026-W40", "2026-09-29"),
    ("2026-W41", "2026-10-06"),
    ("2026-W42", "2026-10-13"),
    ("2026-W43", "2026-10-20"),
    ("2026-W44", "2026-10-27"),
    ("2026-W45", "2026-11-03"),
];

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_until_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("log.parts/History")).unwrap();
    std::fs::write(root.join("log.os"), DOCUMENT).unwrap();
    for (week, day) in WEEKS {
        let handle = format!("w{}", &week[6..]);
        std::fs::write(
            root.join(format!("log.parts/History/{week}.os")),
            format!("list History {{\n    - {{\n        - handle = \"{handle}\"\n        - done_at = \"{day}T08:00:00Z\"\n    }}\n}}\n"),
        )
        .unwrap();
    }
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn ask(documents: &DocumentRoot, list: &str, until: Option<&str>) -> Vec<OverseerNode> {
    let mut args = json!({ "path": "log.os", "list_path": ["log", list] });
    if let Some(key) = until {
        args["until"] = Value::String(key.to_string());
    }
    let answer = documents
        .command_for("alice", Some("log.os"), "show_more_of_overseer_list", &args)
        .expect("asking for more was refused");
    serde_json::from_value(answer["nodes"].clone()).expect("the answer is the whole document")
}

/// The keys a list draws, sorted.
fn drawn(nodes: &[OverseerNode], list: &str, field: &str) -> Vec<String> {
    let list = overseer::addressing::find(nodes, list).expect("the list");
    list.children
        .iter()
        .filter(|e| !overseer::resolver::out_of_view(e))
        .filter_map(|e| e.children.iter().find(|c| c.name == field))
        .filter_map(|h| match h.parameters.get("value") {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn answered(documents: &DocumentRoot, list: &str, until: Option<&str>) -> Value {
    let mut args = json!({ "path": "log.os", "list_path": ["log", list] });
    if let Some(key) = until {
        args["until"] = Value::String(key.to_string());
    }
    documents
        .command_for("alice", Some("log.os"), "show_more_of_overseer_list", &args)
        .expect("asking for more was refused")
}

fn unread(nodes: &[OverseerNode], list: &str) -> i64 {
    match overseer::addressing::find(nodes, list).and_then(|l| l.parameters.get("_parts_left_unread")) {
        Some(OverseerValue::Integer(n)) => *n,
        _ => 0,
    }
}

#[test]
fn a_list_in_parts_reads_the_part_holding_the_key_and_no_other() {
    serialised(|| {
        let (_root, documents) = sandbox("parts");
        let first = documents.open_for("alice", "log.os").expect("open");
        assert_eq!(drawn(&first, "log/History", "handle"), vec!["w45"]);

        let nodes = ask(&documents, "History", Some("w42"));
        assert_eq!(drawn(&nodes, "log/History", "handle"), vec!["w42", "w45"]);
        assert_eq!(unread(&nodes, "log/History"), 4, "parts other than w42's were read");
    });
}

#[test]
fn the_entry_asked_for_stays_drawn_for_that_viewer_alone_through_a_write_elsewhere() {
    serialised(|| {
        let (root, documents) = sandbox("stays");
        let file = root.0.join("log.os");
        // The page listens for the document, as one open on it does.
        let (_, _listening) = overseer::watch::listen(&file);
        documents.open_for("alice", "log.os").expect("open");
        let answer = answered(&documents, "History", Some("w42"));
        let shown = answer["text"].as_str().expect("the answer's text").to_string();

        // Somebody else writes the document, as the agent loop does all afternoon.
        documents
            .command_for(
                "bob",
                Some("log.os"),
                "write_overseer_values",
                &json!({ "path": "log.os", "values": [{ "node_path": ["log", "note"], "value": { "String": "written" } }] }),
            )
            .expect("bob's write was refused");
        let caught_up = documents
            .command_for(
                "alice",
                Some("log.os"),
                "catch_up_overseer_document",
                &json!({ "path": "log.os", "shown_version": overseer::app_api::version_of(&shown) }),
            )
            .expect("the catch-up was refused");
        assert!(
            caught_up.get("changes").is_some_and(|c| !c.is_null()),
            "the catch-up sent the whole document rather than what changed"
        );

        let alice = documents.open_for("alice", "log.os").expect("open again");
        assert_eq!(drawn(&alice, "log/History", "handle"), vec!["w42", "w45"]);
        assert_eq!(unread(&alice, "log/History"), 4);
        let bob = documents.open_for("bob", "log.os").expect("open as bob");
        assert_eq!(drawn(&bob, "log/History", "handle"), vec!["w45"]);
    });
}

#[test]
fn without_a_key_asking_for_more_still_reads_one_window() {
    serialised(|| {
        let (_root, documents) = sandbox("one");
        documents.open_for("alice", "log.os").expect("open");
        let nodes = ask(&documents, "History", None);
        assert_eq!(drawn(&nodes, "log/History", "handle"), vec!["w44", "w45"]);
    });
}

#[test]
fn a_key_drawn_already_draws_nothing_more() {
    serialised(|| {
        let (_root, documents) = sandbox("drawn");
        documents.open_for("alice", "log.os").expect("open");
        let nodes = ask(&documents, "History", Some("w45"));
        assert_eq!(drawn(&nodes, "log/History", "handle"), vec!["w45"]);
    });
}

#[test]
fn a_list_without_parts_draws_the_entry_past_its_window_and_not_those_between() {
    serialised(|| {
        let (_root, documents) = sandbox("window");
        let first = documents.open_for("alice", "log.os").expect("open");
        assert_eq!(drawn(&first, "log/Days", "day"), vec!["mon", "tue"]);
        let nodes = ask(&documents, "Days", Some("thu"));
        assert_eq!(drawn(&nodes, "log/Days", "day"), vec!["mon", "thu", "tue"]);
    });
}

#[test]
fn a_key_no_part_holds_reads_no_part_and_writes_nothing() {
    serialised(|| {
        let (root, documents) = sandbox("nowhere");
        documents.open_for("alice", "log.os").expect("open");
        let nodes = ask(&documents, "History", Some("w99"));
        assert_eq!(drawn(&nodes, "log/History", "handle"), vec!["w45"]);
        assert_eq!(unread(&nodes, "log/History"), 5);
        assert_eq!(std::fs::read_to_string(root.0.join("log.os")).unwrap(), DOCUMENT);
    });
}
