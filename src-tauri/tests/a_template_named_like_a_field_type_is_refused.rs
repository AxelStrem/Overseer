//! A template cannot be named after a field type.
//!
//! An instance's type is its template's name, so an instance of a template named `tags` would be
//! drawn as a tags field. Such a reference is refused, saying why, as one to a missing base is,
//! whether the template is the document's own, another file's, or a list's entry.

use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;
use std::path::PathBuf;

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

fn resolved(text: &str) -> Vec<OverseerNode> {
    let mut nodes = overseer::parser::parse_document(text).expect("parses").1;
    overseer::resolver::resolve_document(&mut nodes);
    nodes
}

fn error(node: &OverseerNode) -> Option<String> {
    match node.parameters.get(overseer::bases::ERROR) {
        Some(OverseerValue::String(why)) => Some(why.clone()),
        _ => None,
    }
}

#[test]
fn an_instance_of_a_template_named_tags_is_refused() {
    let nodes = resolved(
        r##"div (hidden=true) {
    div tags {
        string name = ""
    }
}
tab t (label="T") {
    <tags> mine
}
"##,
    );
    let mine = find(&nodes, "mine").expect("the instance is there");
    assert_ne!(mine.node_type.to_lowercase(), "tags");
    assert_eq!(
        error(mine).as_deref(),
        Some("'tags' is a field type, and cannot name a template")
    );
    assert!(mine.children.is_empty(), "nothing is copied: {:?}", mine.children);
}

#[test]
fn the_name_is_refused_whatever_its_case() {
    let nodes = resolved(
        r##"div (hidden=true) {
    div Button {
        string name = ""
    }
}
tab t (label="T") {
    <Button> mine
}
"##,
    );
    let mine = find(&nodes, "mine").expect("the instance is there");
    assert_ne!(mine.node_type.to_lowercase(), "button");
    assert!(error(mine).is_some());
}

#[test]
fn a_list_whose_entry_is_named_like_a_type_is_refused() {
    let nodes = resolved(
        r##"div (hidden=true) {
    div enum {
        string name = ""
    }
}
tab t (label="T") {
    list Items (entry=<enum>) {
        - {
            - name = "a"
        }
    }
}
"##,
    );
    let items = find(&nodes, "Items").expect("the list is there");
    assert!(error(items).is_some());
    assert!(items
        .children
        .iter()
        .all(|entry| entry.node_type.to_lowercase() != "enum"));
}

#[test]
fn a_base_root_named_like_a_type_is_refused() {
    let root: PathBuf = std::env::temp_dir().join(format!(
        "overseer_typename_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("base.os"),
        "div tags {\n    string name = \"\"\n}\n",
    )
    .unwrap();
    let doc = "tab t (label=\"T\") {\n    <base.os/tags> mine\n}\n";
    let nodes = DocumentManager::with_document(Some(root.clone()), || resolved(doc));
    let mine = find(&nodes, "mine").expect("the instance is there");
    assert_ne!(mine.node_type.to_lowercase(), "tags");
    assert!(error(mine).is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_template_with_any_other_name_still_instantiates() {
    let nodes = resolved(
        r##"div (hidden=true) {
    div Task {
        string name = "x"
    }
}
tab t (label="T") {
    <Task> mine
}
"##,
    );
    let mine = find(&nodes, "mine").expect("the instance is there");
    assert_eq!(mine.node_type, "Task");
    assert!(error(mine).is_none());
    assert!(find(&mine.children, "name").is_some());
}
