//! `host/icons` lists the names of the library's icons, keyed by `tag`, each with its `icon`, so
//! that any field holding an icon's name can pick one from them. It is there only for a document
//! that names it: the others are worked out, and sent, without some 1,500 entries.
#![cfg(feature = "server")]

use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use std::path::PathBuf;

const PICKER: &str = r#"tab profile (mutable=true) {
    enum icon (vocabulary="/host/icons") = "carrot"
    int offered = $(host/icons.count())
    string found = $(host/icons.find(icon)/icon)
}
"#;

const PLAIN: &str = "tab notes (mutable=true) {\n    string kind = $(host/kind)\n}\n";

fn names() -> Vec<String> {
    ["apple", "carrot", "sparkles"].iter().map(|s| s.to_string()).collect()
}

fn served(test: &str) -> (DocumentRoot, PathBuf) {
    let root = std::env::temp_dir().join(format!("overseer-icons-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("profile.os"), PICKER).unwrap();
    std::fs::write(root.join("notes.os"), PLAIN).unwrap();
    // The names are the process's, as the server sets them from the page it serves when it starts.
    overseer::environment::set_icons(names());
    (DocumentRoot::new(&root).unwrap(), root)
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes.iter().find_map(|n| {
        if n.name == name && n.node_type != "context" {
            Some(n)
        } else {
            find(&n.children, name)
        }
    })
}

fn value(nodes: &[OverseerNode], name: &str) -> OverseerValue {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no {}", name));
    assert!(!node.parameters.contains_key(overseer::bases::ERROR), "{}: {:?}", name, node.parameters);
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("{} has no value", name))
}

fn host(nodes: &[OverseerNode]) -> &OverseerNode {
    nodes
        .iter()
        .find(|n| n.node_type == "context" && n.name == "host")
        .expect("the host's context at the top")
}

#[test]
fn a_document_that_names_host_icons_picks_from_them() {
    let (documents, _) = served("picker");
    let nodes = documents.open("profile.os").unwrap();
    assert_eq!(value(&nodes, "offered"), OverseerValue::Integer(3));
    assert_eq!(value(&nodes, "found"), OverseerValue::String("carrot".into()));
    let icons = host(&nodes).children.iter().find(|n| n.name == "icons").expect("host/icons");
    assert_eq!(icons.parameters.get("key"), Some(&OverseerValue::String("tag".into())));
    assert_eq!(icons.children.len(), 3);
}

#[test]
fn a_document_that_does_not_name_them_is_worked_out_without_them() {
    let (documents, _) = served("plain");
    let nodes = documents.open("notes.os").unwrap();
    assert_eq!(value(&nodes, "kind"), OverseerValue::String("browser".into()));
    assert!(host(&nodes).children.iter().all(|n| n.name != "icons"));
}

#[test]
fn the_names_are_read_from_where_the_page_was_built() {
    let page = std::env::temp_dir().join(format!("overseer-icons-page-{}", std::process::id()));
    std::fs::create_dir_all(page.join("icons")).unwrap();
    std::fs::write(page.join("icons").join("index.json"), r#"["a-b","zoom-in"]"#).unwrap();
    assert_eq!(overseer::environment::icon_names_in(&page), vec!["a-b", "zoom-in"]);
    assert!(overseer::environment::icon_names_in(&page.join("nowhere")).is_empty());
}

#[test]
fn a_picked_icon_is_written_and_nothing_of_the_list_with_it() {
    let (documents, root) = served("pick");
    documents
        .set_at("profile.os", "profile/icon", OverseerValue::String("sparkles".into()))
        .expect("the pick");
    assert_eq!(
        std::fs::read_to_string(root.join("profile.os")).unwrap(),
        PICKER.replace("= \"carrot\"", "= \"sparkles\"")
    );
    let nodes = documents.open("profile.os").unwrap();
    assert_eq!(value(&nodes, "found"), OverseerValue::String("sparkles".into()));
}
