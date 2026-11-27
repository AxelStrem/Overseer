//! A vocabulary naming a windowed list with parts leaves the list reading only its newest parts:
//! what its chips are drawn from in the parts not read - each entry's key, title and shape, the
//! template's where the entry writes none - is read from
//! the summaries of those parts and put on the list. A colour worked out from the entry's status
//! is worked out again for each of them; a title worked out from something else than the fields
//! beside it still has the list read whole.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::PathBuf;

fn host(title: &str) -> String {
    host_with(title, r#"context vocabulary = "/log/Items, /log/History""#)
}

fn host_with(title: &str, vocabulary: &str) -> String {
    format!(
        r##"tab log (label="Log", mutable=true) {{
    {vocabulary}

    div (hidden=true) {{
        div Done (layout="horizontal") {{
            string handle = ""
            string title {title}
            string status = "done"
            string colour (hidden=true) = $(/log/Statuses.filter(|s| s/tag == ../status).map(|s| s/colour).first())
            string shape (hidden=true) = "ticket"
            timestamp done_at = "2026-11-02T10:00:00Z"
        }}
        div Status {{
            string tag = ""
            string colour = ""
        }}
        div Item {{
            string handle = ""
            text note = ""
        }}
    }}

    list Statuses (entry=<Status>, key="tag") {{
        - {{
            - tag = "done"
            - colour = "#15803d"
        }}
        - {{
            - tag = "cancelled"
            - colour = "#9f1239"
        }}
    }}

    list Items (entry=<Item>, key="handle") {{
        - {{
            - handle = "p"
            - note = "after <#a>"
        }}
    }}

    list History (entry=<Done>, key="handle", window=1, parts=$(|x| week(x/done_at)),
                  sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) {{ }}
}}
"##
    )
}

const OLDER: &str = r#"list History {
    - {
        - handle = "a"
        - title = "The oldest"
        - done_at = "2026-10-20T08:00:00Z"
    }
    - {
        - handle = "c"
        - status = "cancelled"
        - done_at = "2026-10-21T08:00:00Z"
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


struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, document: &str) -> (Sandbox, DocumentRoot) {
    sandbox_with(tag, document, THIS_WEEK)
}

fn sandbox_with(tag: &str, document: &str, this_week: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_sumvocab_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("log.parts/History")).unwrap();
    std::fs::write(root.join("log.os"), document).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W43.os"), OLDER).unwrap();
    std::fs::write(root.join("log.parts/History/2026-W45.os"), this_week).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn said(nodes: &[OverseerNode], key: &str) -> Option<OverseerValue> {
    overseer::addressing::find(nodes, "log/History")?.parameters.get(key).cloned()
}

fn held(nodes: &[OverseerNode]) -> usize {
    overseer::addressing::find(nodes, "log/History").expect("the list").children.len()
}

#[test]
fn a_vocabulary_is_given_the_entries_of_the_parts_not_read() {
    let _guard = serial();
    let (_sandbox, documents) = sandbox("given", &host("= \"\""));
    let nodes = documents.open("log.os").expect("open");
    assert_eq!(held(&nodes), 1, "only this week is read");
    assert_eq!(said(&nodes, "_parts_read_whole"), None);
    let Some(OverseerValue::String(records)) = said(&nodes, "_parts_vocabulary") else {
        panic!("no vocabulary on the list: {:?}", said(&nodes, "_parts_vocabulary"));
    };
    let records: serde_json::Value = serde_json::from_str(&records).unwrap();
    assert_eq!(
        records,
        serde_json::json!([
            {"key": "a", "title": "The oldest", "colour": "#15803d", "shape": "ticket"},
            {"key": "c", "colour": "#9f1239", "shape": "ticket"},
        ])
    );

    // Worked out again from nothing, the parts are not summarised again.
    let made = overseer::partsums::summaries_made();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    let again = DocumentRoot::new(&_sandbox.0).unwrap();
    let nodes = again.open("log.os").expect("open again");
    assert_eq!(held(&nodes), 1);
    assert_eq!(overseer::partsums::summaries_made(), made, "no part was summarised again");
}

#[test]
fn an_entry_the_window_leaves_out_of_a_part_read_is_given_a_record_too() {
    let _guard = serial();
    let this_week = r#"list History {
    - {
        - handle = "b"
        - title = "This week's"
        - done_at = "2026-11-04T08:00:00Z"
    }
    - {
        - handle = "d"
        - status = "cancelled"
    }
}
"#;
    let (_sandbox, documents) = sandbox_with("leftout", &host("= \"\""), this_week);
    let nodes = documents.open("log.os").expect("open");
    assert_eq!(held(&nodes), 2, "only this week is read");
    let left_out = overseer::addressing::find(&nodes, "log/History/[d]").expect("the entry");
    assert!(overseer::resolver::out_of_view(left_out), "the window leaves it out");
    let Some(OverseerValue::String(records)) = said(&nodes, "_parts_vocabulary") else {
        panic!("no vocabulary on the list: {:?}", said(&nodes, "_parts_vocabulary"));
    };
    let records: serde_json::Value = serde_json::from_str(&records).unwrap();
    assert_eq!(
        records,
        serde_json::json!([
            {"key": "d", "colour": "#9f1239", "shape": "ticket"},
            {"key": "a", "title": "The oldest", "colour": "#15803d", "shape": "ticket"},
            {"key": "c", "colour": "#9f1239", "shape": "ticket"},
        ])
    );
}

#[test]
fn a_title_worked_out_by_a_formula_reads_the_list_whole() {
    let _guard = serial();
    let (_sandbox, documents) = sandbox("formula", &host("= $(handle)"));
    let nodes = documents.open("log.os").expect("open");
    assert_eq!(held(&nodes), 3, "every part is read");
    let Some(OverseerValue::String(why)) = said(&nodes, "_parts_read_whole") else {
        panic!("not read whole");
    };
    assert!(why.contains("vocabulary"), "{}", why);
    assert_eq!(said(&nodes, "_parts_vocabulary"), None);
}

#[test]
fn a_formula_beside_a_written_title_reads_no_old_part() {
    let _guard = serial();
    let (_sandbox, documents) = sandbox("beside", &host("(hidden=$(title == \"\")) = \"\""));
    let nodes = documents.open("log.os").expect("open");
    assert_eq!(held(&nodes), 1, "only this week is read");
    assert_eq!(said(&nodes, "_parts_read_whole"), None);
    let Some(OverseerValue::String(records)) = said(&nodes, "_parts_vocabulary") else {
        panic!("no vocabulary on the list: {:?}", said(&nodes, "_parts_vocabulary"));
    };
    let records: serde_json::Value = serde_json::from_str(&records).unwrap();
    assert_eq!(records[0]["title"], serde_json::json!("The oldest"));
}

#[test]
fn a_marked_element_of_a_vocabulary_block_names_its_list_too() {
    let _guard = serial();
    let block = "context vocabulary {\n        - items = \"/log/Items\"\n        - done = \"%/log/History\"\n    }";
    let (_sandbox, documents) = sandbox("block", &host_with("= \"\"", block));
    let nodes = documents.open("log.os").expect("open");
    assert_eq!(held(&nodes), 1, "only this week is read");
    let Some(OverseerValue::String(records)) = said(&nodes, "_parts_vocabulary") else {
        panic!("no vocabulary on the list: {:?}", said(&nodes, "_parts_vocabulary"));
    };
    let records: serde_json::Value = serde_json::from_str(&records).unwrap();
    assert_eq!(records[0]["key"], serde_json::json!("a"));
}

#[test]
fn a_formula_title_named_by_a_vocabulary_block_reads_the_list_whole() {
    let _guard = serial();
    let block = "context vocabulary {\n        - done = \"%/log/History\"\n    }";
    let (_sandbox, documents) = sandbox("blockformula", &host_with("= $(handle)", block));
    let nodes = documents.open("log.os").expect("open");
    assert_eq!(held(&nodes), 3, "every part is read");
}
