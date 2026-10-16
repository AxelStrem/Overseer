//! An edit to an instance is saved as the instance wrote it, with only the edit changed.
//!
//! The template walk lets an instance write `+` elements, unnamed steps that reach one
//! particular unnamed node, elements the template does not have, and nested overrides of a
//! node's own parameters. The resolved instance holds the whole template around all of that, and
//! rebuilding the instance from it once lost every unnamed step, the nested override and the edit
//! itself. So the writer gives back the instance's own block and splices what changed into it.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::types::*;

const DOC: &str = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Card {
            string name = ""
            tab (label="About") {
                string description = ""
            }
            tab (label="Notes") {
                div {
                    string notes = ""
                }
                div {
                    string links = ""
                }
            }
            div Deep (x=1) {
                string inner = ""
                string other = ""
            }
        }
    }
    // the card
    <Card> card {
        - name = "Overseer"
        +string subtitle = "a language for documents"
        tab { }
        tab {
            div { }
            div (background="#eee") { }
        }
        - Deep (x=2) { }
        - owner = "me"
    }
}
"##;

/// Each edit, and the document as it has to be saved after it: the edit, and nothing else.
fn cases() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "t/card/name",
            "X",
            DOC.replace(r#"- name = "Overseer""#, r#"- name = "X""#),
        ),
        (
            "t/card/subtitle",
            "S",
            DOC.replace(r#"+string subtitle = "a language for documents""#, r#"+string subtitle = "S""#),
        ),
        (
            "t/card/owner",
            "O",
            DOC.replace(r#"- owner = "me""#, r#"- owner = "O""#),
        ),
        (
            "t/card/description",
            "D",
            DOC.replace(
                "        tab { }\n",
                "        tab {\n            - description = \"D\"\n        }\n",
            ),
        ),
        (
            "t/card/notes",
            "N",
            DOC.replace(
                "            div { }\n",
                "            div {\n                - notes = \"N\"\n            }\n",
            ),
        ),
        (
            "t/card/links",
            "L",
            DOC.replace(
                "            div (background=\"#eee\") { }\n",
                "            div (background=\"#eee\") {\n                - links = \"L\"\n            }\n",
            ),
        ),
        (
            "t/card/Deep/inner",
            "I",
            DOC.replace(
                "        - Deep (x=2) { }\n",
                "        - Deep (x=2) {\n            - inner = \"I\"\n        }\n",
            ),
        ),
    ]
}

fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> Option<&'a OverseerNode> {
    let (first, rest) = path.split_first()?;
    fn below<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
        }
        // Through what the address does not name.
        nodes
            .iter()
            .filter(|n| n.name.is_empty() || n.name == n.node_type || n.is_hierarchy_transparent)
            .find_map(|n| below(&n.children, name))
    }
    let node = below(nodes, first)?;
    if rest.is_empty() {
        Some(node)
    } else {
        find(&node.children, rest)
    }
}

fn value_at(text: &str, path: &str) -> Option<OverseerValue> {
    let nodes = app_api::load_document(text.to_string()).unwrap();
    let parts: Vec<&str> = path.split('/').collect();
    find(&nodes, &parts)?.parameters.get("value").cloned()
}

fn background_of_the_links_div(text: &str) -> Option<OverseerValue> {
    let nodes = app_api::load_document(text.to_string()).unwrap();
    let card = find(&nodes, &["t", "card"])?;
    let notes_tab = card.children.iter().filter(|c| c.node_type == "tab").nth(1)?;
    notes_tab.children.get(1)?.parameters.get("background").cloned()
}

fn saved_after(text: &str, path: &str, value: &str) -> String {
    let mut changed = std::collections::HashMap::new();
    changed.insert(path.to_string(), OverseerValue::String(value.to_string()));
    let nodes = app_api::resolve_selective(text.to_string(), vec![path.to_string()], Some(changed))
        .unwrap();
    OverseerFileHandler::serialize_nodes(&nodes).unwrap()
}

fn a_root(tag: &str, text: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_edited_instance_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("t.os"), text).expect("write");
    root
}

fn written_through_the_server(tag: &str, text: &str, address: &str, value: OverseerValue) -> String {
    let root = a_root(tag, text);
    let service = DocumentRoot::new(&root).expect("open the root");
    service.set_at("t.os", address, value).expect("the write was refused");
    std::fs::read_to_string(root.join("t.os")).expect("read")
}

#[test]
fn an_unedited_instance_saves_as_it_was_written() {
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DOC);
}

#[test]
fn each_edit_changes_only_its_own_line_from_the_page() {
    for (path, value, expected) in cases() {
        let saved = saved_after(DOC, path, value);
        assert_eq!(saved, expected, "after an edit to {}", path);
        assert_eq!(
            value_at(&saved, path),
            Some(OverseerValue::String(value.to_string())),
            "{} did not read back",
            path
        );
        assert_eq!(
            background_of_the_links_div(&saved),
            Some(OverseerValue::String("#eee".to_string())),
            "the unnamed step stopped landing after an edit to {}",
            path
        );
        // Saved once more, it stays as it is.
        let nodes = app_api::load_document(saved.clone()).unwrap();
        assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), saved);
    }
}

#[test]
fn each_edit_changes_only_its_own_line_from_the_server() {
    for (i, (path, value, expected)) in cases().into_iter().enumerate() {
        let saved = written_through_the_server(
            &format!("server{}", i),
            DOC,
            path,
            OverseerValue::String(value.to_string()),
        );
        assert_eq!(saved, expected, "after a write to {}", path);
    }
}

/// What a `set` action with `target="/t/card/Deep.x"` does, saved.
fn parameter_set(text: &str, target: &str, value: OverseerValue) -> String {
    let mut nodes = app_api::load_document(text.to_string()).unwrap();
    overseer::actions::ActionExecutor::assign_value(&mut nodes, target, value).unwrap();
    OverseerFileHandler::serialize_nodes(&nodes).unwrap()
}

#[test]
fn a_nested_parameter_becomes_an_override_where_it_is() {
    // `x` was overridden by the instance: the override's own header is the one that changes.
    let saved = parameter_set(DOC, "/t/card/Deep.x", OverseerValue::Integer(3));
    assert_eq!(saved, DOC.replace("- Deep (x=2) { }", "- Deep (x=3) { }"));

    // Without that override, the edit is one: the container named, the parameter in its header.
    let plain = DOC.replace("        - Deep (x=2) { }\n", "");
    let saved = parameter_set(&plain, "/t/card/Deep.x", OverseerValue::Integer(3));
    assert_eq!(
        saved,
        plain.replace(
            "            div (background=\"#eee\") { }\n        }\n",
            "            div (background=\"#eee\") { }\n        }\n        - Deep (x=3)\n",
        )
    );
    let nodes = app_api::load_document(saved).unwrap();
    let deep = find(&nodes, &["t", "card", "Deep"]).unwrap();
    assert_eq!(deep.parameters.get("x"), Some(&OverseerValue::Integer(3)));
    assert!(find(&nodes, &["t", "card", "Deep", "inner"]).is_some(), "the template's fields went");
}

#[test]
fn an_edit_with_nothing_written_around_it_goes_after_what_was() {
    let plain = DOC.replace(
        "        tab { }\n        tab {\n            div { }\n            div (background=\"#eee\") { }\n        }\n",
        "",
    );
    let saved = saved_after(&plain, "t/card/links", "L");
    assert_eq!(
        saved,
        plain.replace(
            "        - name = \"Overseer\"\n        +string subtitle = \"a language for documents\"\n",
            "        - name = \"Overseer\"\n        +string subtitle = \"a language for documents\"\n        - links = \"L\"\n",
        )
    );
    assert_eq!(value_at(&saved, "t/card/links"), Some(OverseerValue::String("L".to_string())));
}
