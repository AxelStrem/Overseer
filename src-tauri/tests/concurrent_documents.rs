//! Two documents must be able to resolve at the same time.
//!
//! A `mount` writes its source relative to the document that declares it, so resolving one
//! needs to know which document is being resolved. That answer used to be a single
//! process-wide slot: with two documents in play, whichever loaded last decided where both
//! of their mounts lived. In the app that is harmless - one document is open - but it is the
//! first thing that breaks when something serves several at once, and it breaks quietly, by
//! reading the wrong file rather than by failing.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(f) = find(&n.children, name) {
            return Some(f);
        }
    }
    None
}

/// A document that mounts a catalogue sitting beside it.
fn write_pair(dir: &std::path::Path, marker: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("catalogue.os"),
        format!(
            "tab catalogue (mutable=true) {{\n    div Items (layout=\"vertical\") {{\n        string marker = \"{}\"\n    }}\n}}\n",
            marker
        ),
    )
    .unwrap();
    let doc = dir.join("doc.os");
    std::fs::write(
        &doc,
        "tab doc (mutable=true) {\n    mount CAT (hidden=true, lazy=false, mutable=false, source=\"catalogue.os/catalogue/Items\") { }\n}\n",
    )
    .unwrap();
    doc
}

fn marker_of(nodes: &[OverseerNode]) -> Option<String> {
    match find(nodes, "marker")?.parameters.get("value")? {
        OverseerValue::String(s) => Some(s.clone()),
        _ => None,
    }
}

#[test]
fn each_document_resolves_its_own_mount() {
    let root = std::env::temp_dir().join(format!("overseer_ctx_{}", std::process::id()));
    let alpha_doc = write_pair(&root.join("alpha"), "ALPHA");
    let beta_doc = write_pair(&root.join("beta"), "BETA");

    let read = |doc: std::path::PathBuf, expected: &'static str| {
        let text = std::fs::read_to_string(&doc).unwrap();
        let dir = DocumentManager::dir_of(doc.to_string_lossy().as_ref());
        let nodes = DocumentManager::with_document(dir, || app_api::load_document(text)).unwrap();
        assert_eq!(
            marker_of(&nodes).as_deref(),
            Some(expected),
            "the document resolved its mount against the wrong directory"
        );
    };

    // Sequentially first: each has to be right on its own before concurrency means anything.
    read(alpha_doc.clone(), "ALPHA");
    read(beta_doc.clone(), "BETA");

    // Now at the same time, which is what a server does.
    let a = std::thread::spawn(move || read(alpha_doc, "ALPHA"));
    let b = std::thread::spawn(move || read(beta_doc, "BETA"));
    a.join().expect("alpha resolved the wrong catalogue");
    b.join().expect("beta resolved the wrong catalogue");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_scope_does_not_outlive_the_work_it_was_opened_for() {
    let root = std::env::temp_dir().join(format!("overseer_ctx_nest_{}", std::process::id()));
    let doc = write_pair(&root.join("alpha"), "ALPHA");
    let dir = DocumentManager::dir_of(doc.to_string_lossy().as_ref());

    assert!(
        DocumentManager::scoped_base_dir().is_none(),
        "a scope was left open before this test began"
    );
    DocumentManager::with_document(dir, || {
        assert!(DocumentManager::scoped_base_dir().is_some());
    });
    assert!(
        DocumentManager::scoped_base_dir().is_none(),
        "the scope outlived the work it was opened for"
    );

    let _ = std::fs::remove_dir_all(&root);
}
