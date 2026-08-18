//! The browser's way of writing: run the action, serialize, send the document back.
//!
//! Finer-grained writes exist for the bot, but nothing in the page speaks them - a button
//! pressed in a browser is an ordinary save of the whole document.

use overseer::server::{DocumentRoot, RequestError};

struct Sandbox {
    root: std::path::PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_save_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("doc.os"),
        "tab t (label=\"T\", mutable=true) {\n    int amount = 1\n}\n",
    )
    .unwrap();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox { root }, documents)
}

fn text(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("doc.os")).unwrap()
}

const CHANGED: &str = "tab t (label=\"T\", mutable=true) {\n    int amount = 7\n}\n";

#[test]
fn a_document_can_be_saved_from_the_page() {
    let (sandbox, documents) = sandbox("plain");
    let original = text(&sandbox.root);
    documents
        .command(
            None,
            "save_overseer_file_with_original",
            &serde_json::json!({ "path": "doc.os", "regenerated": CHANGED, "original": original }),
        )
        .expect("save");
    assert_eq!(text(&sandbox.root), CHANGED);
}

#[test]
fn a_stale_page_cannot_discard_what_was_written_since() {
    let (sandbox, documents) = sandbox("stale");
    let stale_baseline = text(&sandbox.root);

    // Something else writes - the bot recording a meal, say.
    let meanwhile = "tab t (label=\"T\", mutable=true) {\n    int amount = 2\n}\n";
    std::fs::write(sandbox.root.join("doc.os"), meanwhile).unwrap();

    let outcome = documents.command(
        None,
        "save_overseer_file_with_original",
        &serde_json::json!({ "path": "doc.os", "regenerated": CHANGED, "original": stale_baseline }),
    );
    match outcome {
        Err(RequestError::Rejected(message)) => {
            assert!(message.contains("changed since this page loaded it"), "{}", message)
        }
        Err(other) => panic!("refused for the wrong reason: {:?}", other),
        Ok(_) => panic!("a stale save was allowed"),
    }
    assert_eq!(text(&sandbox.root), meanwhile, "the other write was lost");
}

#[test]
fn saving_stays_inside_the_document_root() {
    let (_sandbox, documents) = sandbox("escape");
    let outcome = documents.command(
        None,
        "save_overseer_file",
        &serde_json::json!({ "path": "../escaped.os", "content": CHANGED }),
    );
    assert!(matches!(outcome, Err(RequestError::Rejected(_))), "a path outside the root was accepted");
}

#[test]
fn an_unknown_save_command_is_refused() {
    let (_sandbox, documents) = sandbox("unknown");
    let outcome = documents.command(
        None,
        "save_everything_everywhere",
        &serde_json::json!({ "path": "doc.os" }),
    );
    assert!(matches!(outcome, Err(RequestError::Rejected(_))));
}
