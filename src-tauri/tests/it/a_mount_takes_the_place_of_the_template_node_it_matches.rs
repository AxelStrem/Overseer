//! A template can hold an empty default where an instance may mount another file: the instance's
//! `mount` of the same name takes the template node's place whole, and is brought in as any mount
//! is - on a first open, after a write, and after a restart alike. An instance without the mount
//! reads the default.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const PEOPLE: &str = r#"div (hidden=true) {
    div Person {
        string tag = ""
    }
}

list People (entry=<Person>, key="tag") {
    - {
        - tag = "ann"
    }
    - {
        - tag = "bob"
    }
}
"#;

const BASE: &str = r#"div (hidden=true) {
    div Book {
        string title = ""
    }
}

tab shelf (mutable=true) {
    div (hidden=true) {
        div people {
            list People { }
        }
    }
    int readers = $(/shelf/people/People.count())
    list Books (entry=<Book>, key="title") {
    }
}
"#;

const MOUNTED: &str = r#"<base.os/shelf> shelf {
    mount people (source="people.os/People", lazy=false, hidden=true, mutable=false) { }
    - Books {
        - {
            - title = "Dune"
        }
    }
}
"#;

const UNMOUNTED: &str = r#"<base.os/shelf> shelf {
    - Books {
        - {
            - title = "Dune"
        }
    }
}
"#;

fn folder(test: &str, document: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_mount_over_template_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (name, text) in [("people.os", PEOPLE), ("base.os", BASE), ("d.os", document)] {
        std::fs::write(root.join(name), text).unwrap();
    }
    root
}

/// The documents as a server just started finds them, nothing held from before.
fn opened(root: &Path) -> (DocumentRoot, Vec<OverseerNode>) {
    overseer::document_disk::settled();
    overseer::partsums::forget_in_memory();
    overseer::parts::let_go_of_idle_parts(std::time::Duration::ZERO);
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(root).unwrap();
    let nodes = documents.open("d.os").expect("open");
    (documents, nodes)
}

fn readers(nodes: &[OverseerNode]) -> Option<OverseerValue> {
    let params = &overseer::addressing::find(nodes, "shelf/readers")?.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

/// What stands at `shelf/people`: its type, and the tags of the people it holds.
fn people(nodes: &[OverseerNode]) -> (String, Vec<String>) {
    let at = overseer::addressing::find(nodes, "shelf/people").expect("shelf/people");
    let list = overseer::addressing::find(nodes, "shelf/people/People").expect("shelf/people/People");
    let tags = list
        .children
        .iter()
        .filter_map(|entry| entry.children.iter().find(|c| c.name == "tag"))
        .filter_map(|tag| match tag.parameters.get("_computed_value").or_else(|| tag.parameters.get("value")) {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    (at.node_type.clone(), tags)
}

fn mounted() -> (String, Vec<String>) {
    ("mount".to_string(), vec!["ann".to_string(), "bob".to_string()])
}

#[test]
fn a_mount_over_a_template_node_is_brought_in_on_the_first_open() {
    let _guard = serial();
    let root = folder("first", MOUNTED);
    let (_documents, nodes) = opened(&root);
    assert_eq!(people(&nodes), mounted());
    assert_eq!(readers(&nodes), Some(OverseerValue::Integer(2)));
}

#[test]
fn a_mount_over_a_template_node_reads_the_same_after_a_write_and_after_a_restart() {
    let _guard = serial();
    let root = folder("write", MOUNTED);
    let (documents, _) = opened(&root);
    let fields: HashMap<String, OverseerValue> =
        [("title".to_string(), OverseerValue::String("Emma".into()))].into_iter().collect();
    documents.append_at("d.os", "shelf/Books", &fields).expect("append");

    let written = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(
        written,
        MOUNTED.replace(
            "            - title = \"Dune\"\n        }\n",
            "            - title = \"Dune\"\n        }\n        - {\n            - title = \"Emma\"\n        }\n",
        ),
        "the mount is written back where the instance wrote it, and only the entry is added"
    );
    assert_eq!(std::fs::read_to_string(root.join("base.os")).unwrap(), BASE);

    let after_the_write = documents.open("d.os").expect("open after the write");
    assert_eq!(people(&after_the_write), mounted());
    assert_eq!(readers(&after_the_write), Some(OverseerValue::Integer(2)));

    let (_documents, restarted) = opened(&root);
    assert_eq!(people(&restarted), mounted());
    assert_eq!(readers(&restarted), Some(OverseerValue::Integer(2)));
}

#[test]
fn an_instance_without_the_mount_reads_the_template_default() {
    let _guard = serial();
    let root = folder("default", UNMOUNTED);
    let (_documents, nodes) = opened(&root);
    assert_eq!(people(&nodes), ("div".to_string(), Vec::new()));
    assert_eq!(readers(&nodes), Some(OverseerValue::Integer(0)));
}
