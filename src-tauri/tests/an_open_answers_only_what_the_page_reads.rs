//! What the page is sent is what it reads, and nothing it does not.
//!
//! A node carries where it came from in the text - its parameters' order, its place among its
//! siblings, the blank lines before it, a fingerprint of its source - and its parameters carry
//! notes on what a template supplied and what is overridden. The serializer needs all of it and
//! the disk cache keeps it; the page never sends a document back to be serialized, so none of it
//! goes to the page. It was most of every open's answer.

use overseer::server::DocumentRoot;
use serde_json::{json, Value};
use std::path::PathBuf;

const PANTRY: &str = r#"// What is in the pantry
tab pantry {
    div (hidden=true) {
        div Jar {
            string name = ""
            int count = 1
        }
    }

    list jars (entry=<Jar>, key="name") {
        - Jar { name = "beans" }
        - Jar { name = "rice" count = 3 }
    }

    - total = $(jars.map(|j| j/count).sum())
}
"#;

fn a_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_slimanswer_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("pantry.os"), PANTRY).expect("write");
    root
}

const NODE_BOOKKEEPING: [&str; 7] = [
    "param_order",
    "child_original_index",
    "leading_blank_lines",
    "source_id",
    "source_fingerprint",
    "raw_value_literal",
    "authored_dash",
];

/// Everything in an answer that only writing the document back needs, by where it was found.
fn bookkeeping_in(value: &Value, at: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if map.contains_key("node_type") {
                for key in NODE_BOOKKEEPING {
                    if map.contains_key(key) {
                        found.push(format!("{} on {}", key, at));
                    }
                }
                if let Some(Value::Object(params)) = map.get("parameters") {
                    for key in params.keys() {
                        let for_the_serializer = (key.starts_with("_template_") && key != "_template_type")
                            || key == "_explicit_child_override"
                            || key == "_override_present"
                            || key == "_explicit_overrides";
                        if for_the_serializer {
                            found.push(format!("{} on {}", key, at));
                        }
                    }
                }
            }
            let name = map.get("name").and_then(Value::as_str).unwrap_or("");
            for inner in map.values() {
                bookkeeping_in(inner, &format!("{}/{}", at, name), found);
            }
        }
        Value::Array(items) => {
            for item in items {
                bookkeeping_in(item, at, found);
            }
        }
        _ => {}
    }
}

fn open(service: &DocumentRoot) -> Value {
    service
        .command_for(
            "alice",
            Some("pantry.os"),
            "parse_overseer_content",
            &json!({ "content": PANTRY, "path": "pantry.os" }),
        )
        .expect("open the pantry")
}

#[test]
fn an_open_carries_none_of_the_serializers_bookkeeping() {
    let root = a_root("open");
    let service = DocumentRoot::new(&root).expect("open the root");
    let answer = open(&service);
    let mut found = Vec::new();
    bookkeeping_in(&answer, "", &mut found);
    assert!(found.is_empty(), "the open carried bookkeeping: {:?}", &found[..found.len().min(10)]);
}

#[test]
fn an_open_still_carries_what_the_page_reads() {
    let root = a_root("reads");
    let service = DocumentRoot::new(&root).expect("open the root");
    let answer = open(&service);
    let text = answer.to_string();
    // A list entry still says what it was made from, and a formula what it worked out to.
    assert!(text.contains("\"_original_type\""), "an entry no longer says what it was made from");
    assert!(text.contains("\"_computed_value\":{\"Integer\":4}"), "the total was not worked out: {}", text);
    assert!(text.contains("\"rice\""), "an entry's value is missing");
}

#[test]
fn a_change_carries_none_of_it_either() {
    let root = a_root("change");
    let service = DocumentRoot::new(&root).expect("open the root");
    open(&service);
    let nodes = service.open("pantry.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, "pantry/jars/[rice]/count").expect("the count");
    let answer = service
        .command_for(
            "alice",
            Some("pantry.os"),
            "write_overseer_values",
            &json!({
                "path": "pantry.os",
                "values": [{ "node_path": path, "value": { "Integer": 5 } }]
            }),
        )
        .expect("write a count");
    let mut found = Vec::new();
    bookkeeping_in(&answer, "", &mut found);
    assert!(found.is_empty(), "the change carried bookkeeping: {:?}", &found[..found.len().min(10)]);
    // And the file was written from its own text, comment and all.
    let written = std::fs::read_to_string(root.join("pantry.os")).expect("read back");
    assert!(written.starts_with("// What is in the pantry\n"), "the comment was lost: {}", written);
    assert!(written.contains("count = 5"), "the count was not written: {}", written);
}

#[test]
fn the_document_itself_still_keeps_it() {
    // The disk cache stores the nodes the ordinary way, and needs every field back.
    let nodes = overseer::app_api::load_document(PANTRY.to_string()).expect("resolve");
    let plain = serde_json::to_string(&nodes).expect("serialize");
    assert!(plain.contains("\"source_fingerprint\""));
    assert!(plain.contains("\"_template_"));
    let slim = serde_json::to_string(&overseer::types::ForThePage(&nodes)).expect("serialize");
    assert!(!slim.contains("\"source_fingerprint\""));
    assert!(slim.len() < plain.len());
    // And writing for the page leaves nothing switched on behind it.
    assert_eq!(serde_json::to_string(&nodes).expect("serialize"), plain);
}
