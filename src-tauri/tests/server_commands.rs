//! What the server will answer, for a frontend running in a browser.
//!
//! The frontend talks through a single call, so it can address this server exactly as it
//! addresses the desktop app. The commands are the desktop's and are handled by the same
//! `app_api` code - that is what stops a document behaving one way in the app and another in
//! a browser. These tests are about the boundary: what is refused, and what a command needs
//! to be told that the desktop could take for granted.

use overseer::server::{DocumentRoot, RequestError};
use serde_json::json;

fn examples() -> DocumentRoot {
    DocumentRoot::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples"))
        .expect("examples directory")
}

const TRACKER: &str = "weight_tracker/tracker_v2.os";

fn text_of(name: &str) -> String {
    let root = examples();
    let value = root
        .command(Some(name), "load_overseer_file", &json!({ "path": name }))
        .expect("the document should be readable");
    value.as_str().expect("text").to_string()
}

#[test]
fn resolves_a_document_and_what_it_mounts() {
    // Unlike the app, a server has several documents in play, so it has to be told which one
    // this is - a mount is written relative to the document that declares it.
    let resolved = examples()
        .command(
            Some(TRACKER),
            "parse_overseer_content",
            &json!({ "content": text_of(TRACKER) }),
        )
        .expect("the document should resolve");
    let encoded = serde_json::to_string(&resolved).unwrap();
    assert!(
        encoded.contains("Catalog"),
        "the mounted catalogue did not resolve, so the mount read nothing"
    );
}

#[test]
fn refuses_to_resolve_a_document_it_has_not_been_told_the_name_of() {
    let outcome = examples().command(None, "parse_overseer_content", &json!({ "content": "tab t {}" }));
    assert!(
        matches!(outcome, Err(RequestError::Rejected(_))),
        "resolving without a named document was allowed: {:?}",
        outcome
    );
}

#[test]
fn refuses_to_save() {
    // Reading and writing are separate matters, and this server does not write yet. Refused
    // outright rather than left to fail somewhere less obvious.
    for cmd in [
        "save_overseer_file",
        "save_overseer_file_with_original",
        "save_overseer_file_from_text",
    ] {
        let outcome = examples().command(Some(TRACKER), cmd, &json!({}));
        assert!(
            matches!(outcome, Err(RequestError::Rejected(_))),
            "'{}' was not refused: {:?}",
            cmd,
            outcome
        );
    }
}

#[test]
fn refuses_a_command_it_does_not_have() {
    let outcome = examples().command(Some(TRACKER), "rm_rf", &json!({}));
    assert!(matches!(outcome, Err(RequestError::Rejected(_))));
}

#[test]
fn refuses_to_read_a_file_outside_the_documents_it_serves() {
    // The command takes a path of its own, so it is checked the same way a document name is.
    for path in ["../Cargo.toml", "weight_tracker/../../Cargo.toml"] {
        let outcome = examples().command(
            Some(TRACKER),
            "load_overseer_file",
            &json!({ "path": path }),
        );
        assert!(
            outcome.is_err(),
            "'{}' was read even though it sits outside the served documents",
            path
        );
    }
}

#[test]
fn an_edit_is_answered_with_what_changed() {
    let root = examples();
    let text = text_of(TRACKER);
    // Establish what the caller holds, as opening the document does.
    root.command(Some(TRACKER), "parse_overseer_content", &json!({ "content": text.clone() }))
        .expect("open");

    let answer = root
        .command(
            Some(TRACKER),
            "parse_overseer_content_selective_update",
            &json!({ "content": text, "changedFields": [], "changedFieldValues": {} }),
        )
        .expect("the edit should resolve");
    assert!(
        answer.get("text").is_some(),
        "no text came back for the next interaction"
    );
    assert!(
        answer.get("changes").is_some() || answer.get("nodes").is_some(),
        "the answer described neither a change nor a document"
    );
}
