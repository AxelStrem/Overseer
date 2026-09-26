//! The last entry of a list can be removed.
//!
//! A node with no children is written by replaying its block as it was first read - which keeps a
//! comment inside an empty list, and a missing block missing. But a list that held entries and has
//! lost them all is not the block that was authored, and replaying it put back exactly what had
//! been removed: taking the only entry off answered Ok and left the file unchanged. Found through
//! the drawer's list of documents, which starts with one entry and loses it as readily.

use overseer::app_api;

const WITH_ONE: &str = "tab t (mutable=true) {\n\n    div (hidden=true) {\n        div Item {\n            string name = \"\"\n        }\n    }\n\n    list Items (entry=<Item>, key=\"name\") {\n        - {\n            - name = \"only\"\n        }\n    }\n\n    // Kept: a list that was always empty, with a note inside it.\n    list Notes (entry=string) {\n        // nothing yet\n    }\n}\n";

#[test]
fn removing_the_only_entry_leaves_the_list_empty() {
    let root = std::env::temp_dir().join(format!("overseer_last_entry_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("t.os"), WITH_ONE).unwrap();
    let nodes = app_api::load_document(WITH_ONE.to_string()).unwrap();
    let entry = overseer::addressing::name_path(&nodes, "t/Items/[only]").expect("the entry");
    app_api::remove_entry_at(&root.join("t.os").to_string_lossy(), "t.os", entry, "s").expect("remove");
    let after = std::fs::read_to_string(root.join("t.os")).unwrap();
    assert!(!after.contains("\"only\""), "the entry came back:\n{}", after);
    assert!(after.contains("    list Items (entry=<Item>, key=\"name\") {\n    }\n"), "{}", after);
    assert!(after.contains("        // nothing yet\n"), "the note in an empty list was lost:\n{}", after);
}
