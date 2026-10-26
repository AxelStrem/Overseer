//! `me` is a context of the environment, so it is there whether or not settings are in force: empty
//! where nobody is signed in. A document worked out bare - the library used on its own, as most
//! tests do - reads `me/handle` as "" and a button that reads it still presses.

use overseer::app_api;

const EXAMPLE: &str = include_str!("../../examples/projects/project.os");

const WHO: &str = "tab t {\n    string who = $(me/handle)\n    string called = $(me/name)\n}\n";

#[test]
fn me_reads_as_empty_in_a_document_worked_out_with_no_settings() {
    assert!(overseer::environment::file().is_none());
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(WHO.to_string()).expect("open");
    for name in ["t/who", "t/called"] {
        let node = overseer::addressing::node_at_path(&nodes, name).expect(name);
        assert!(
            !node.parameters.contains_key(overseer::bases::ERROR),
            "{}: {:?}",
            name,
            node.parameters
        );
        assert_eq!(
            node.parameters.get("_computed_value"),
            Some(&overseer::types::OverseerValue::String(String::new())),
            "{}",
            name
        );
    }
}

#[test]
fn a_button_that_reads_me_presses_with_no_settings_in_force() {
    let root = std::env::temp_dir().join(format!("overseer-me-bare-{}", std::process::id()));
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
