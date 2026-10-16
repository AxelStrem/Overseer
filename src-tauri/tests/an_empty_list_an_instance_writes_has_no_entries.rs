//! A list an instance writes with no entries has none, not the template's.
//!
//! A list's entries are its data, and an instance that writes a list replaces the template's
//! entries with its own. The merge only did so when the instance's list had entries, so a
//! document that wrote `- Items { }` over a template holding sample entries showed the samples -
//! and a list emptied by an edit got them back once the file was read again.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::*;

const TEMPLATE: &str = "    div (hidden=true) {\n        div Item {\n            string name = \"\"\n        }\n        div Project {\n            string title = \"\"\n            list Items (entry=<Item>, key=\"name\") {\n                - {\n                    - name = \"sample\"\n                }\n            }\n        }\n    }\n";

fn document(instance: &str) -> String {
    format!("tab t (mutable=true) {{\n{}\n{}}}\n", TEMPLATE, instance)
}

fn entries(nodes: &[OverseerNode]) -> Vec<String> {
    let node = overseer::addressing::node_at_path(nodes, "t/p/Items").expect("the list");
    node.children
        .iter()
        .map(|entry| {
            entry
                .children
                .iter()
                .find(|c| c.name == "name")
                .and_then(|c| c.parameters.get("value"))
                .map(|v| format!("{:?}", v))
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn an_instance_that_writes_an_empty_list_has_no_entries() {
    let text = document("    <Project> p {\n        - title = \"Empty\"\n        - Items {\n        }\n    }\n");
    let nodes = app_api::load_document(text.clone()).unwrap();
    assert!(entries(&nodes).is_empty(), "the template's entries came through: {:?}", entries(&nodes));
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), text);
}

#[test]
fn an_instance_that_does_not_write_the_list_has_the_templates_entries() {
    let text = document("    <Project> p {\n        - title = \"Full\"\n    }\n");
    let nodes = app_api::load_document(text.clone()).unwrap();
    assert_eq!(entries(&nodes).len(), 1);
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), text);
}

#[test]
fn an_instance_that_only_sets_a_lists_parameters_has_the_templates_entries() {
    let text = document("    <Project> p {\n        - Items (label=\"Mine\")\n    }\n");
    let nodes = app_api::load_document(text.clone()).unwrap();
    assert_eq!(entries(&nodes).len(), 1);
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), text);
}

#[test]
fn a_list_emptied_by_an_edit_stays_empty_when_read_again() {
    let text = document(
        "    <Project> p {\n        - Items {\n            - {\n                - name = \"mine\"\n            }\n        }\n    }\n",
    );
    let root = std::env::temp_dir().join(format!("overseer_empty_list_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("t.os"), &text).unwrap();
    let nodes = app_api::load_document(text.clone()).unwrap();
    let entry = overseer::addressing::name_path(&nodes, "t/p/Items/[mine]").expect("the entry");
    app_api::remove_entry_at(&root.join("t.os").to_string_lossy(), "t.os", entry, "s").expect("remove");
    let after = std::fs::read_to_string(root.join("t.os")).unwrap();
    assert!(!after.contains("\"mine\""), "the entry came back:\n{}", after);
    let again = app_api::load_document(after.clone()).unwrap();
    assert!(entries(&again).is_empty(), "the samples came back after the edit:\n{}", after);
}
