//! Asking a list for more `until` a key reads back as many windows as it takes to draw the entry
//! with that key, and no further: a chip pressed for a closed item weeks back opens the history
//! on it, without the rest of the history being read.

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

fn unread(nodes: &[OverseerNode], list: &str) -> i64 {
    match overseer::addressing::find(nodes, list).and_then(|l| l.parameters.get("_parts_left_unread")) {
        Some(OverseerValue::Integer(n)) => *n,
        _ => 0,
    }
}

#[test]
fn a_list_in_parts_reads_back_to_the_part_holding_the_key_and_no_further() {
    serialised(|| {
        let (_root, documents) = sandbox("parts");
        let first = documents.open_for("alice", "log.os").expect("open");
        assert_eq!(drawn(&first, "log/History", "handle"), vec!["w45"]);

        let nodes = ask(&documents, "History", Some("w42"));
        assert_eq!(drawn(&nodes, "log/History", "handle"), vec!["w42", "w43", "w44", "w45"]);
        assert_eq!(unread(&nodes, "log/History"), 2, "the parts older than w42's are still unread");
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
fn a_key_drawn_already_asks_for_one_window_only() {
    serialised(|| {
        let (_root, documents) = sandbox("drawn");
        documents.open_for("alice", "log.os").expect("open");
        let nodes = ask(&documents, "History", Some("w45"));
        assert_eq!(drawn(&nodes, "log/History", "handle"), vec!["w44", "w45"]);
    });
}

#[test]
fn a_list_without_parts_draws_entries_past_its_window_until_the_key() {
    serialised(|| {
        let (_root, documents) = sandbox("window");
        let first = documents.open_for("alice", "log.os").expect("open");
        assert_eq!(drawn(&first, "log/Days", "day"), vec!["mon", "tue"]);
        let nodes = ask(&documents, "Days", Some("thu"));
        assert_eq!(drawn(&nodes, "log/Days", "day"), vec!["mon", "thu", "tue", "wed"]);
    });
}

#[test]
fn a_key_nowhere_reads_the_list_to_its_end_and_writes_nothing() {
    serialised(|| {
        let (root, documents) = sandbox("nowhere");
        documents.open_for("alice", "log.os").expect("open");
        let nodes = ask(&documents, "History", Some("w99"));
        assert_eq!(drawn(&nodes, "log/History", "handle").len(), 6);
        assert_eq!(unread(&nodes, "log/History"), 0);
        assert_eq!(std::fs::read_to_string(root.0.join("log.os")).unwrap(), DOCUMENT);
    });
}
