//! An item added to a project with `+ task` belongs to whoever added it: its `owner` is their
//! username, drawn as their chip. Where nobody is signed in - the desktop app - it is left empty,
//! and the add still goes through. Both work under settings, as the app and the server always do:
//! `me` is declared with them.

use overseer::app_api;

const EXAMPLE: &str = include_str!("../../examples/projects/project.os");

// One at a time: what a document was worked out under is held for the whole process.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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

    let answer = overseer::environment::with_file(root.join("settings.os"), || {
        app_api::run_event_at(&file.to_string_lossy(), "p.os", add, "click".into(), "s", Vec::new(), Vec::new())
    })
    .expect("the add");
    assert!(answer.wrote);
    let written = std::fs::read_to_string(&file).unwrap();
    assert_eq!(written.matches("- {").count(), before + 1, "{}", written);
    assert!(!written.contains("- owner"), "{}", written);
}
