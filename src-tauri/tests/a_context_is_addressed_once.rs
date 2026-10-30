//! A context is addressed once, by its declaration.
//!
//! A container that declares a context holds, once worked out, a copy of it as well as the
//! declaration - see `contexts`. The copy is what a formula inside meets; it is never written, and
//! it is no part of the container's contents for anyone outside. It used to take the plain
//! address, pushing the declaration to `theme#1`: an entry read over the server listed every
//! context its template declares twice, and a write to the declaration's own address failed.

use overseer::server::DocumentRoot;
use overseer::types::*;

const DOCUMENT: &str = r#"div (hidden=true) {
    div Item {
        string name = ""
        context hide-labels = true
        string seen (label="Seen") = "x"
    }
}
tab t (label="T", mutable=true) {
    list items (entry=<Item>, key="name") {
        - {
            - name = "first"
        }
    }
    div plain {
        context theme {
            string accent = "blue"
        }
        string tint = $(theme/accent)
    }
}
"#;

struct Sandbox {
    root: std::path::PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_context_once_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("doc.os"), DOCUMENT).unwrap();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox { root }, documents)
}

fn resolved() -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = overseer::parser::parse_document(DOCUMENT).expect("parse").1;
    overseer::resolver::resolve_document(&mut nodes);
    nodes
}

#[test]
fn every_context_has_one_address() {
    let nodes = resolved();
    let addresses = overseer::addressing::addresses(&nodes);
    for wanted in ["t/items/[first]/hide-labels", "t/plain/theme", "t/plain/theme/accent"] {
        assert_eq!(
            addresses.iter().filter(|a| *a == wanted).count(),
            1,
            "`{}` should be addressed once: {:?}",
            wanted,
            addresses
        );
    }
    assert!(
        addresses.iter().all(|a| !a.contains('#')),
        "nothing here shares a name with anything else: {:?}",
        addresses
    );
}

#[test]
fn a_change_to_what_a_container_sees_still_reaches_the_page() {
    // The settings in force decide which copies the box holds: under a theme, one of it as well.
    // The tests beside this one make a server, which sets the process's settings while this runs,
    // so this one names its own - a file that is not there, which reads as the default theme - for
    // its thread alone.
    let nowhere = std::env::temp_dir().join(format!("overseer_context_once_none_{}", std::process::id()));
    overseer::environment::with_file(nowhere.join("settings.os"), || {
        a_change_to_what_a_container_sees_reaches_the_page_under_the_settings_in_force()
    });
}

fn a_change_to_what_a_container_sees_reaches_the_page_under_the_settings_in_force() {
    let resolve = |accent: &str| {
        overseer::source_registry::SourceRegistry::reset();
        let source = format!("div box {{\n    context accent = \"{}\"\n    string tint = $(accent)\n}}\n", accent);
        let mut nodes = overseer::parser::parse_document(&source).expect("parse").1;
        overseer::resolver::resolve_document(&mut nodes);
        nodes
    };
    let before = resolve("blue");
    let after = resolve("red");
    let copy_at = after[0]
        .children
        .iter()
        .position(|child| overseer::contexts::is_copy(child) && child.name == "accent")
        .expect("the box holds the copy the page reads");
    let changed: Vec<(String, Vec<usize>)> = overseer::delta::diff(&before, &after)
        .iter()
        .filter_map(|change| match change {
            overseer::delta::DocumentChange::Parameters { address, path, .. } => {
                Some((address.clone(), path.clone()))
            }
            _ => None,
        })
        .collect();
    assert!(
        changed.contains(&("box/(accent)".to_string(), vec![0, copy_at])),
        "the copy the page reads changed and was not said to: {:?}",
        changed
    );
    assert!(changed.iter().any(|(address, _)| address == "box/accent"), "{:?}", changed);
}

#[test]
fn a_contexts_address_is_its_declaration() {
    let nodes = resolved();
    let found = overseer::addressing::find(&nodes, "t/plain/theme").expect("the theme");
    assert!(!overseer::contexts::is_copy(found), "the address reached the worked-out copy");
}

#[test]
fn an_entry_read_over_the_server_lists_its_context_once() {
    let (_sandbox, documents) = sandbox("read");
    let view = documents.read_at("doc.os", "t/items/[first]").expect("the entry");
    assert_eq!(
        view.child_addresses,
        ["t/items/[first]/name", "t/items/[first]/hide-labels", "t/items/[first]/seen"]
    );
    assert_eq!(
        view.node.children.len(),
        view.child_addresses.len(),
        "a caller pairs the children with their addresses by place"
    );
}

#[test]
fn a_write_to_a_contexts_address_reaches_the_file() {
    let (sandbox, documents) = sandbox("write");
    documents
        .set_at("doc.os", "t/plain/theme/accent", OverseerValue::String("red".into()))
        .expect("the declaration's field should be writable at its address");
    let text = std::fs::read_to_string(sandbox.root.join("doc.os")).unwrap();
    assert!(text.contains(r#"string accent = "red""#), "{}", text);
    let view = documents.read_at("doc.os", "t/plain/tint").expect("the tint");
    assert_eq!(
        view.node.parameters.get("_computed_value"),
        Some(&OverseerValue::String("red".into()))
    );
}
