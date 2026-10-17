//! A list's header may be written over several lines. Moving or adding an entry changes the
//! entries' lines and nothing of the header's.

use overseer::app_api;
use overseer::types::OverseerValue;

const DOCUMENT: &str = r##"tab t (label="T", mutable=true) {

    list Items (entry=<Item>, key="name",
                movable=true) {
        - {
            - name = "first"
        }
        - {
            - name = "second"
        }
    }

    div Item (hidden=true) {
        string name = ""
    }
}
"##;

fn opened(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_wrapped_header_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("t.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    file.to_string_lossy().to_string()
}

#[test]
fn a_moved_entry_leaves_the_header_on_its_two_lines() {
    let path = opened("move");
    let nodes = app_api::load_document(std::fs::read_to_string(&path).unwrap()).unwrap();
    let entry = overseer::addressing::name_path(&nodes, "t/Items/[first]").expect("the entry");
    let list = overseer::addressing::name_path(&nodes, "t/Items").expect("the list");
    app_api::move_entry_at(&path, "t.os", entry, list, None, None, "s").expect("the move");

    let expected = DOCUMENT
        .replace("- name = \"first\"", "- name = \"TMP\"")
        .replace("- name = \"second\"", "- name = \"first\"")
        .replace("- name = \"TMP\"", "- name = \"second\"");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
}

#[test]
fn an_added_entry_leaves_the_header_on_its_two_lines() {
    let path = opened("append");
    let mut fields = std::collections::HashMap::new();
    fields.insert("name".to_string(), OverseerValue::String("third".into()));
    app_api::append_entry_at(&path, "t.os", vec!["t".into(), "Items".into()], fields, "s").expect("the append");

    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("list Items (entry=<Item>, key=\"name\",\n                movable=true) {\n"), "{}", saved);
    assert!(saved.contains("- name = \"third\""), "{}", saved);
}
