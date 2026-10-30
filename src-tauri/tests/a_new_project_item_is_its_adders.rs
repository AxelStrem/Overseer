//! An item added to a project with `+ task` belongs to whoever added it: its `owner` is their
//! username, drawn as their chip. Where nobody is signed in - the desktop app - it is left empty,
//! and the add still goes through, with or without settings in force.
//!
//! It starts flagged `manual`, for a person to work: agents file theirs with `tracker.py`, never
//! through the form, so the flag says which items a person added. Emptying the form's flags box
//! hands the item to the agents.

use overseer::app_api;

const EXAMPLE: &str = include_str!("../../examples/projects/project.os");

#[cfg(feature = "server")]
fn served(test: &str) -> (overseer::server::DocumentRoot, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("overseer-owner-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("ap31")).unwrap();
    std::fs::write(root.join("ap31/p.os"), EXAMPLE).unwrap();
    let accounts = overseer::accounts::Accounts::open(root.join("accounts.json"))
        .and_then(|a| a.with_directory(&root))
        .unwrap();
    let now = chrono::Utc::now();
    let invite = accounts.invite(None, false, now).unwrap();
    accounts.join(&invite, "ap31", "Dmitrii", "a long password", now).unwrap();
    let documents = overseer::server::DocumentRoot::new(&root).unwrap().with_accounts(|name| name == "ap31");
    (documents, root)
}

#[cfg(feature = "server")]
#[test]
fn an_item_added_by_a_person_is_theirs() {
    let (documents, root) = served("person");
    documents.as_asker(overseer::access::Asker::Person("ap31".into()), || {
        documents
            .command_for(
                "ap31",
                Some("ap31/p.os"),
                "run_overseer_event",
                &serde_json::json!({ "node_path": ["project", "NewTask", "add"], "event_name": "click" }),
            )
            .expect("the add");
    });
    let written = std::fs::read_to_string(root.join("ap31/p.os")).unwrap();
    assert_eq!(written.matches("- owner = \"ap31\"").count(), 1, "{}", written);
}

#[test]
fn an_item_added_where_nobody_is_signed_in_has_no_owner() {
    let root = std::env::temp_dir().join(format!("overseer-owner-nobody-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, EXAMPLE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(EXAMPLE.to_string()).expect("open");
    let add = overseer::addressing::name_path(&nodes, "project/NewTask/add").expect("the add button");
    let before = EXAMPLE.matches("- {").count();

    let answer =
        app_api::run_event_at(&file.to_string_lossy(), "p.os", add, "click".into(), "s", Vec::new(), Vec::new())
            .expect("the add");
    assert!(answer.wrote);
    let written = std::fs::read_to_string(&file).unwrap();
    assert_eq!(written.matches("- {").count(), before + 1, "{}", written);
    assert!(!written.contains("- owner"), "{}", written);
}

/// Adds one item from the form, with what is typed into its boxes, and gives back the new item as
/// the file writes it.
fn added_with(test: &str, typed: &[(&str, &str)]) -> String {
    let root = std::env::temp_dir().join(format!("overseer-owner-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, EXAMPLE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(EXAMPLE.to_string()).expect("open");
    let add = overseer::addressing::name_path(&nodes, "project/NewTask/add").expect("the add button");
    let typed = typed
        .iter()
        .map(|(field, text)| app_api::ValueWrite {
            node_path: vec!["project".into(), "NewTask".into(), field.to_string()],
            value: overseer::types::OverseerValue::String(text.to_string()),
        })
        .collect();

    app_api::run_event_at(&file.to_string_lossy(), "p.os", add, "click".into(), "s", typed, Vec::new())
        .expect("the add");
    let written = std::fs::read_to_string(&file).unwrap();
    let at = written.find("- title = \"A new one\"").expect("the new item");
    let start = written[..at].rfind("- {").unwrap();
    let end = start + written[start..].find("\n        }").unwrap();
    written[start..end].to_string()
}

#[test]
fn an_item_added_in_the_page_is_for_a_person_to_work() {
    let item = added_with("manual", &[("title", "A new one")]);
    assert!(item.contains("- flags = \"manual\""), "{}", item);
}

#[test]
fn an_item_whose_flags_were_emptied_goes_to_the_agents() {
    let item = added_with("unflagged", &[("title", "A new one"), ("flags", "")]);
    assert!(!item.contains("- flags"), "{}", item);
}

#[test]
fn flags_typed_into_the_form_are_the_items() {
    let item = added_with("first", &[("title", "A new one"), ("flags", "first")]);
    assert!(item.contains("- flags = \"first\""), "{}", item);
}
