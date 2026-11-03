//! An open of a list with parts sends the page what it draws - how many parts were left unread,
//! and what a vocabulary takes of them - and none of what the server keeps to read and write its
//! parts: the folder they are in, and every part's file with its stamp. Those are paths on the
//! server's disk, which no one opening the document needs to be told.
//!
//! A lambda the list states - `parts`, `sort_by` - is applied to its entries where it is used,
//! and is not worked out as a value of its own.

use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::Value;
use std::path::PathBuf;

const LOG: &str = r#"tab log (label="Log", mutable=true) {
    context vocabulary = "/log/History"

    div (hidden=true) {
        div Done (layout="horizontal") {
            string handle = ""
            string title = ""
            timestamp done_at = "2026-11-02T10:00:00Z"
        }
    }

    list History (entry=<Done>, key="handle", window=1, parts=$(|x| week(x/done_at)),
                  sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) { }
}
"#;

const OLDER: &str = r#"list History {
    - {
        - handle = "a"
        - title = "The oldest"
        - done_at = "2026-10-20T08:00:00Z"
    }
}
"#;

const THIS_WEEK: &str = r#"list History {
    - {
        - handle = "b"
        - title = "This week's"
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_partpaths_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("log.parts/History")).unwrap();
    std::fs::write(root.join("log.os"), LOG).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W43.os"), OLDER).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W45.os"), THIS_WEEK).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

/// The document as the server answers an open of it.
fn answered(documents: &DocumentRoot) -> Value {
    let nodes = documents.open("log.os").expect("open");
    serde_json::to_value(ForThePage(&nodes)).expect("serialize")
}

/// The parameters of the node at the end of a path of names, in an answer.
fn params_at<'a>(nodes: &'a Value, path: &[&str]) -> &'a serde_json::Map<String, Value> {
    let mut level = nodes.as_array().expect("nodes");
    let mut found = None;
    for step in path {
        let node = level
            .iter()
            .find(|n| n["name"] == *step)
            .unwrap_or_else(|| panic!("no {} in the answer", step));
        level = node["children"].as_array().expect("children");
        found = Some(node);
    }
    found.unwrap()["parameters"].as_object().expect("parameters")
}

#[test]
fn an_open_names_no_file_on_the_servers_disk() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = sandbox("paths");
    let text = answered(&documents).to_string();
    let folder = sandbox.0.file_name().unwrap().to_string_lossy().into_owned();
    assert!(!text.contains(&folder), "the answer names the server's folder");
    assert!(!text.contains(".parts"), "the answer names the parts folder");
    assert!(!text.contains("2026-W43.os"), "the answer names a part's file");
}

#[test]
fn an_open_carries_none_of_the_parts_bookkeeping() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("bookkeeping");
    let answer = answered(&documents);
    let list = params_at(&answer, &["log", "History"]);
    let kept: Vec<&String> = list
        .keys()
        .filter(|k| k.starts_with("_parts_") && *k != "_parts_left_unread" && *k != "_parts_vocabulary")
        .collect();
    assert!(kept.is_empty(), "the list carried the server's bookkeeping: {:?}", kept);
}

#[test]
fn an_open_still_carries_what_the_page_draws_of_the_parts() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("draws");
    let answer = answered(&documents);
    let list = params_at(&answer, &["log", "History"]);
    assert_eq!(list.get("_parts_left_unread"), Some(&serde_json::json!({ "Integer": 1 })), "the note under the list");
    let vocabulary = list.get("_parts_vocabulary").map(Value::to_string).unwrap_or_default();
    assert!(vocabulary.contains("The oldest"), "the chips of the part not read: {}", vocabulary);
}

#[test]
fn a_lambda_a_list_states_is_not_worked_out_as_a_value() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = sandbox("lambda");
    let nodes = documents.open("log.os").expect("open");
    let list = overseer::addressing::find(&nodes, "log/History").expect("the list");
    for key in ["_computed_parts", "_computed_sort_by"] {
        assert_eq!(list.parameters.get(key), None, "{} was worked out", key);
    }
}
