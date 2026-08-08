//! What a document server will and will not look up.
//!
//! A document name arrives from the network. The interesting cases are not the ones that
//! work, but the ones that must not: reaching outside the directory being served, by any of
//! the several spellings that amount to the same thing.

use overseer::server::{DocumentRoot, RequestError};

fn examples() -> DocumentRoot {
    DocumentRoot::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples"),
    )
    .expect("examples directory")
}

#[test]
fn lists_documents_by_the_name_a_caller_would_use() {
    let names = examples().list();
    assert!(
        names.contains(&"weight_tracker/tracker_v2.os".to_string()),
        "expected a nested document in {:?}",
        &names[..names.len().min(8)]
    );
    assert!(
        names.iter().all(|n| n.ends_with(".os")),
        "something other than a document was listed"
    );
    assert!(
        names.iter().all(|n| !n.chars().any(|c| c == '\u{5c}')),
        "names must be spelled the way they are asked for, got {:?}",
        names.iter().find(|n| n.chars().any(|c| c == '\u{5c}'))
    );
}

#[test]
fn opens_a_document_and_resolves_what_it_mounts() {
    // tracker_v2 mounts foods.os beside it; if the mount did not resolve, the catalogue-backed
    // fields would be errors rather than values.
    let nodes = examples()
        .open("weight_tracker/tracker_v2.os")
        .expect("tracker_v2 should open");
    fn find(nodes: &[overseer::types::OverseerNode], name: &str) -> bool {
        nodes
            .iter()
            .any(|n| n.name == name || find(&n.children, name))
    }
    assert!(find(&nodes, "FOODS"), "the mount is missing from the document");
    assert!(
        find(&nodes, "Catalog"),
        "the mounted catalogue did not resolve, so the mount read nothing"
    );
}

#[test]
fn refuses_to_look_outside_the_directory_it_serves() {
    let root = examples();
    for name in [
        "../Cargo.toml",
        "../src-tauri/Cargo.toml",
        "weight_tracker/../../Cargo.toml",
        "weight_tracker/../../src-tauri/src/main.rs",
    ] {
        let outcome = root.resolve(name);
        assert!(
            matches!(outcome, Err(RequestError::Rejected(_)) | Err(RequestError::NotFound(_))),
            "'{}' was resolved to {:?}",
            name,
            outcome
        );
    }
}

#[test]
fn refuses_an_absolute_path() {
    let root = examples();
    for name in ["/etc/passwd", "C:/Windows/System32/drivers/etc/hosts"] {
        assert!(
            root.resolve(name).is_err(),
            "'{}' was accepted as a document name",
            name
        );
    }
}

#[test]
fn serves_only_documents() {
    // Not a general file server: whatever else sits in the directory stays there.
    let outcome = examples().resolve("weight_tracker/foods.txt");
    assert!(
        matches!(outcome, Err(RequestError::Rejected(_))),
        "a non-document was not refused: {:?}",
        outcome
    );
}

#[test]
fn a_missing_document_is_missing_rather_than_an_error() {
    assert!(matches!(
        examples().open("weight_tracker/no_such_file.os"),
        Err(RequestError::NotFound(_))
    ));
}
