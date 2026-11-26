//! A person names their own documents short - `day.os` - or in full - `ap31/day.os` - and both are
//! one document: the page is told the full name to show, and what is kept for the viewer is kept
//! once, whichever was asked.

use crate::common::shared;
use overseer::access::Asker;
use overseer::server::{DocumentRoot, RequestError};
use overseer::types::{OverseerNode, OverseerValue};
use serde_json::json;
use std::path::Path;

const SHARING: &str = r#"div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "day.os"
        - with = "bob"
    }
}
"#;

const DAY: &str = r#"tab day (mutable=true) {
    int shown (mutable="guarded") = 1

    button next {
        on click {
            set (path="../shown") = $(../shown + 1)
        }
    }
}
"#;

fn write(root: &Path, name: &str, text: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn people(test: &str) -> DocumentRoot {
    let root = std::env::temp_dir().join(format!("overseer-full-name-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(&root, "ap31/sharing.os", SHARING);
    write(&root, "ap31/day.os", DAY);
    write(&root, "ap31/private.os", "string entry = \"a secret\"\n");
    write(&root, "journal.os", "string said = \"the server's\"\n");
    DocumentRoot::new(&root)
        .unwrap()
        .with_accounts(|name| name == "ap31" || name == "bob")
}

fn person(name: &str) -> Asker {
    Asker::Person(name.into())
}

/// The full name the page is told for `document`, as `asker`.
fn told(documents: &DocumentRoot, asker: Asker, document: &str) -> Result<serde_json::Value, RequestError> {
    documents.as_asker(asker, || {
        documents
            .command(Some(document), "access", &json!({}))
            .map(|answer| answer["document"].clone())
    })
}

fn shown(nodes: &[OverseerNode]) -> Option<OverseerValue> {
    let node = overseer::addressing::find(nodes, "day/shown")?;
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

#[test]
fn the_page_is_told_the_full_name_of_the_document_it_opened() {
    let _shared = shared();
    let documents = people("told");
    assert_eq!(told(&documents, person("ap31"), "day.os"), Ok(json!("ap31/day.os")));
    assert_eq!(told(&documents, person("ap31"), "ap31/day.os"), Ok(json!("ap31/day.os")));
    // Shared with Bob, it is named as its owner's, which is the only name he has for it.
    assert_eq!(told(&documents, person("bob"), "ap31/day.os"), Ok(json!("ap31/day.os")));
    // Nothing is told of what may not be opened: it answers as what is not there does.
    assert_eq!(
        told(&documents, person("bob"), "ap31/private.os"),
        Err(RequestError::NotFound("no document 'ap31/private.os'".into()))
    );
    // The token works in the whole root: its names are already the full ones.
    assert_eq!(told(&documents, Asker::Anyone, "journal.os"), Ok(json!("journal.os")));
    assert_eq!(told(&documents, Asker::Anyone, "ap31/day.os"), Ok(json!("ap31/day.os")));
}

#[test]
fn what_a_viewer_looks_at_is_the_same_at_either_name() {
    let _shared = shared();
    let documents = people("viewer");
    let session = format!("full-name-{}", std::process::id());
    let press = json!({ "node_path": ["day", "next"], "event_name": "click" });
    documents.as_asker(person("ap31"), || {
        documents
            .command_for(&session, Some("day.os"), "run_overseer_event", &press)
            .expect("pressed at the short name");
        let full = documents.open_for(&session, "ap31/day.os").expect("opened at the full name");
        assert_eq!(shown(&full), Some(OverseerValue::Integer(2)));

        documents
            .command_for(&session, Some("ap31/day.os"), "run_overseer_event", &press)
            .expect("pressed at the full name");
        let short = documents.open_for(&session, "day.os").expect("opened at the short name");
        assert_eq!(shown(&short), Some(OverseerValue::Integer(3)));
    });
    assert!(overseer::viewstate::overlay(&session, "day.os").is_empty(), "kept under the short name too");
    assert!(!overseer::viewstate::overlay(&session, "ap31/day.os").is_empty());
    let file = std::fs::read_to_string(documents.path().join("ap31/day.os")).unwrap();
    assert_eq!(file, DAY, "what the viewer looks at reached the file");
    overseer::viewstate::forget(&session, "ap31/day.os");
}
