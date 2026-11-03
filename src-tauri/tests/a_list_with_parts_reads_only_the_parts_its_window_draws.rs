//! A list with `parts` and a `window` reads its parts from the newest back, only as far as the
//! window needs, and an older part only when "more" reaches it - or when something reads the list
//! whole, or reaches an entry it has not read.
//!
//! A history in weekly parts is most of a project's document, and what is drawn of it is the last
//! few entries. A part nothing reaches is never parsed, so nothing in its entries is worked out.

use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const TEMPLATE: &str = r#"    div (hidden=true) {
        div Done (layout="horizontal") {
            string handle = ""
            timestamp done_at = "2026-11-02T10:00:00Z"
            int points = 1
            int twice = $(points * 2)
        }
    }
"#;

fn host(extra: &str) -> String {
    format!(
        r#"tab log (label="Log", mutable=true) {{
{TEMPLATE}{extra}
    list History (entry=<Done>, key="handle", window=1, parts=$(|x| week(x/done_at)),
                  sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) {{ }}

    button redo (label="Move a to this week") {{
        on click {{
            set_in_list (list="../History", keyField="handle", keyValue="a", field="points") = 9
        }}
    }}
}}
"#
    )
}

const OLDEST: &str = r#"list History {
    - {
        - handle = "a"
        - done_at = "2026-10-20T08:00:00Z"
        - points = 3
    }
}
"#;

const OLDER: &str = r#"list History {
    - {
        - handle = "c"
        - done_at = "2026-10-27T08:00:00Z"
        - points = 4
    }
}
"#;

const THIS_WEEK: &str = r#"list History {
    - {
        - handle = "b"
        - points = 5
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, document: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_partdraw_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("log.parts/History")).unwrap();
    std::fs::write(root.join("log.os"), document).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W43.os"), OLDEST).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W44.os"), OLDER).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W45.os"), THIS_WEEK).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn on_disk(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).unwrap_or_else(|_| "(no file)".to_string())
}

fn value(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let params = &overseer::addressing::find(nodes, address)?.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

/// The handles the list holds, in its order.
fn held(nodes: &[OverseerNode], list: &str) -> Vec<String> {
    let list = overseer::addressing::find(nodes, list).expect("the list");
    list.children
        .iter()
        .filter_map(|e| e.children.iter().find(|c| c.name == "handle"))
        .filter_map(|h| match h.parameters.get("value") {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn unread(nodes: &[OverseerNode], list: &str) -> i64 {
    match overseer::addressing::find(nodes, list).and_then(|l| l.parameters.get("_parts_left_unread")) {
        Some(OverseerValue::Integer(n)) => *n,
        _ => 0,
    }
}

fn done(handle: &str, at: &str, points: i64) -> HashMap<String, OverseerValue> {
    [
        ("handle".to_string(), OverseerValue::String(handle.to_string())),
        ("done_at".to_string(), OverseerValue::Timestamp(at.to_string())),
        ("points".to_string(), OverseerValue::Integer(points)),
    ]
    .into_iter()
    .collect()
}

#[test]
fn a_windowed_list_reads_only_its_newest_parts() {
    serialised(|| {
        let (sandbox, documents) = sandbox("newest", &host(""));
        // Were it read, this part would be named as unreadable.
        std::fs::write(sandbox.0.join("log.parts/History/2026-W43.os"), "list History {\n    - {\n").unwrap();
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(held(&nodes, "log/History"), vec!["b"], "only this week is read");
        assert_eq!(unread(&nodes, "log/History"), 2, "and the list says how many parts it left");
        assert_eq!(value(&nodes, "log/History/[b]/twice"), Some(OverseerValue::Integer(10)));
        let list = overseer::addressing::find(&nodes, "log/History").unwrap();
        assert!(list.parameters.get("_parts_problem").is_none(), "a part never parsed is never a problem: {:?}", list.parameters.get("_parts_problem"));
    });
}

#[test]
fn asking_for_more_reads_the_next_part() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("more", &host(""));
        documents.open_for("alice", "log.os").expect("open");
        let answer = documents
            .command_for("alice", Some("log.os"), "show_more_of_overseer_list", &json!({ "path": "log.os", "list_path": ["log", "History"] }))
            .expect("asking for more");
        let nodes: Vec<OverseerNode> = serde_json::from_value(answer["nodes"].clone()).expect("the whole document");
        assert_eq!(held(&nodes, "log/History"), vec!["c", "b"], "one window more reads the week before");
        assert_eq!(unread(&nodes, "log/History"), 1);
        assert_eq!(value(&nodes, "log/History/[c]/twice"), Some(OverseerValue::Integer(8)), "and works it out");
        let theirs = documents.open("log.os").expect("open for anyone else");
        assert_eq!(held(&theirs, "log/History"), vec!["b"], "anyone else still sees the window the document states");
    });
}

#[test]
fn a_formula_naming_the_list_reads_every_part() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("total", &host("    int total (mutable=false) = $(History.sum(|x| x/points))\n"));
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/total"), Some(OverseerValue::Integer(12)));
        assert_eq!(held(&nodes, "log/History"), vec!["a", "c", "b"], "in the order of the parts");
        assert_eq!(unread(&nodes, "log/History"), 0);
    });
}

#[test]
fn a_read_of_an_old_entry_finds_it_worked_out() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("read", &host(""));
        documents.open("log.os").expect("open");
        let view = documents.read_at("log.os", "log/History/[a]/twice").expect("the old entry is found");
        let params = &view.node.parameters;
        assert_eq!(params.get("_computed_value").or_else(|| params.get("value")), Some(&OverseerValue::Integer(6)));
    });
}

#[test]
fn a_read_of_the_list_reads_all_of_it() {
    serialised(|| {
        let (_sandbox, documents) = sandbox("readlist", &host(""));
        let view = documents.read_at("log.os", "log/History").expect("the list");
        assert_eq!(held(std::slice::from_ref(&view.node), "History"), vec!["a", "c", "b"]);
    });
}

#[test]
fn a_set_in_a_part_not_read_writes_only_that_part() {
    serialised(|| {
        let (sandbox, documents) = sandbox("set", &host(""));
        documents.open("log.os").expect("open");
        documents.set_at("log.os", "log/History/[c]/points", OverseerValue::Integer(7)).expect("the set");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLDER.replace("4", "7"));
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W43.os"), OLDEST);
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os"), THIS_WEEK);
        assert_eq!(on_disk(&sandbox.0, "log.os"), host(""));
    });
}

#[test]
fn an_append_to_a_part_not_read_keeps_what_the_part_held() {
    serialised(|| {
        let (sandbox, documents) = sandbox("append_old", &host(""));
        documents.open("log.os").expect("open");
        documents.append_at("log.os", "log/History", &done("d", "2026-10-21T12:00:00Z", 2)).expect("the append");
        assert_eq!(
            on_disk(&sandbox.0, "log.parts/History/2026-W43.os"),
            "list History {\n    - {\n        - handle = \"a\"\n        - done_at = \"2026-10-20T08:00:00Z\"\n        - points = 3\n    }\n    - {\n        - handle = \"d\"\n        - done_at = \"2026-10-21T12:00:00Z\"\n        - points = 2\n    }\n}\n",
        );
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLDER, "a part the write did not reach is left as it was");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os"), THIS_WEEK);

        // And a second write, from the document as the first left it, keeps them too.
        documents.append_at("log.os", "log/History", &done("e", "2026-10-22T12:00:00Z", 1)).expect("a second append");
        let oldest = on_disk(&sandbox.0, "log.parts/History/2026-W43.os");
        for handle in ["\"a\"", "\"d\"", "\"e\""] {
            assert!(oldest.contains(handle), "{} is still there: {}", handle, oldest);
        }
        documents.undo("log.os").expect("undo");
        documents.undo("log.os").expect("undo");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W43.os"), OLDEST, "two undos put it back");
    });
}

#[test]
fn an_append_to_this_week_leaves_the_parts_not_read_alone() {
    serialised(|| {
        let (sandbox, documents) = sandbox("append", &host(""));
        documents.open("log.os").expect("open");
        documents.append_at("log.os", "log/History", &done("f", "2026-11-04T12:00:00Z", 2)).expect("the append");
        assert!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os").contains("- handle = \"f\""));
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W43.os"), OLDEST);
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLDER);
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(held(&nodes, "log/History"), vec!["b", "f"]);
    });
}

#[test]
fn a_press_reaching_an_entry_not_read_finds_it() {
    serialised(|| {
        let (sandbox, documents) = sandbox("press", &host(""));
        documents.open("log.os").expect("open");
        documents.run_event("log.os", "log/redo", "click").expect("the press");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W43.os"), OLDEST.replace("3", "9"));
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLDER);
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os"), THIS_WEEK);
    });
}

#[test]
fn a_new_part_on_disk_is_drawn() {
    serialised(|| {
        let (sandbox, documents) = sandbox("appears", &host(""));
        documents.open("log.os").expect("open");
        std::fs::write(
            sandbox.0.join("log.parts/History/2026-W46.os"),
            "list History {\n    - {\n        - handle = \"g\"\n        - done_at = \"2026-11-10T08:00:00Z\"\n    }\n}\n",
        )
        .unwrap();
        let nodes = documents.open("log.os").expect("reopen");
        assert_eq!(held(&nodes, "log/History"), vec!["g"]);
        assert_eq!(unread(&nodes, "log/History"), 3);
    });
}

#[test]
fn entries_the_document_held_all_go_to_their_parts_window_or_not() {
    serialised(|| {
        let held = host("").replace(
            "sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) { }",
            "sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) {\n        - {\n            - handle = \"h\"\n            - done_at = \"2026-10-21T08:00:00Z\"\n        }\n        - {\n            - handle = \"i\"\n        }\n    }",
        );
        let (sandbox, documents) = sandbox("held", &held);
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(value(&nodes, "log/History/[i]/twice"), Some(OverseerValue::Integer(2)), "an entry not in its part yet is worked out");
        documents.set_at("log.os", "log/History/[b]/points", OverseerValue::Integer(6)).expect("a write to the list");
        assert_eq!(on_disk(&sandbox.0, "log.os"), host("").replace("{ }", "{\n    }"));
        assert!(on_disk(&sandbox.0, "log.parts/History/2026-W43.os").contains("\"h\""), "to a part it had not read");
        assert!(on_disk(&sandbox.0, "log.parts/History/2026-W43.os").contains("\"a\""), "which keeps what it held");
        assert!(on_disk(&sandbox.0, "log.parts/History/2026-W45.os").contains("\"i\""), "and by its template's date");
        assert_eq!(on_disk(&sandbox.0, "log.parts/History/2026-W44.os"), OLDER);
    });
}

const BASE: &str = r#"div (hidden=true) {
    div Done (layout="horizontal") {
        string handle = ""
        timestamp done_at = "2026-11-02T10:00:00Z"
        int points = 1
        int twice = $(points * 2)
    }
    tab log (label="Log", mutable=true) {
        string name = ""
        list History (entry=<Done>, key="handle", window=1, parts=$(|x| week(x/done_at)),
                      sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) { }
    }
}
"#;

#[test]
fn a_list_a_template_gives_is_windowed_over_its_parts() {
    serialised(|| {
        let (sandbox, documents) = sandbox("instance", "<base.os/log> log {\n    - name = \"Mine\"\n}\n");
        std::fs::write(sandbox.0.join("base.os"), BASE).unwrap();
        let nodes = documents.open("log.os").expect("open");
        assert_eq!(held(&nodes, "log/History"), vec!["b"], "the window applies to a list a template gave");
        assert_eq!(unread(&nodes, "log/History"), 2);
        assert_eq!(value(&nodes, "log/History/[b]/twice"), Some(OverseerValue::Integer(10)));
    });
}

const UNPARTED_BASE: &str = r#"div (hidden=true) {
    div Done (layout="horizontal") {
        string handle = ""
        int points = 1
        int twice = $(points * 2)
    }
    tab log (label="Log", mutable=true) {
        list History (entry=<Done>, key="handle", window=1) {
            - { - handle = "a" }
            - { - handle = "b" }
            - { - handle = "c" }
        }
    }
}
"#;

#[test]
fn a_window_on_a_list_a_template_gives_leaves_the_rest_unworked() {
    serialised(|| {
        let (sandbox, documents) = sandbox("instance_window", "<base.os/log> log {\n}\n");
        std::fs::write(sandbox.0.join("base.os"), UNPARTED_BASE).unwrap();
        let nodes = documents.open("log.os").expect("open");
        let list = overseer::addressing::find(&nodes, "log/History").expect("the list");
        assert_eq!(list.parameters.get("_left_out_of_view"), Some(&OverseerValue::Integer(2)));
        let worked: Vec<bool> = list.children.iter().map(|e| !e.parameters.contains_key("_not_worked_out")).collect();
        assert_eq!(worked, vec![true, false, false]);
    });
}
