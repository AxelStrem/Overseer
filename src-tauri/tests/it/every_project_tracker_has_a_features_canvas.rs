//! Every project tracker has a Features tab: a list of features drawn as a canvas, inherited from
//! the shipped template without the tracker writing anything for it. The tab is unnamed, inside
//! `project`, so the list is `project/Features` beside `project/Items`.
//!
//! A feature added waits unplaced, a standing picked stamps when it was touched, and a card
//! dragged writes both of its coordinates in one step.

use crate::common::serial;
use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;

const EXAMPLE: &str = include_str!("../../../examples/projects/project.os");


fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = serial();
    let out = body();
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

fn a_project(tag: &str, text: &str) -> (std::path::PathBuf, String) {
    let root = std::env::temp_dir().join(format!("overseer_features_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, text).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-05T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    (root, file.to_string_lossy().to_string())
}

fn on_disk(file: &str) -> String {
    std::fs::read_to_string(file).unwrap()
}

fn path_of(nodes: &[OverseerNode], address: &str) -> Vec<String> {
    overseer::addressing::name_path(nodes, address).unwrap_or_else(|| panic!("no {}", address))
}

/// The lines of one feature, from its handle to the brace that closes it.
fn feature_lines(text: &str, handle: &str) -> Vec<String> {
    let at = text.find(&format!("- handle = \"{}\"", handle)).expect(handle);
    let open = text[..at].rfind("- {").expect("the entry's opening");
    text[open + 3..]
        .lines()
        .map(|l| l.trim().to_string())
        .take_while(|l| l != "}")
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn a_tracker_that_never_names_the_tab_has_it_empty() {
    serialised(|| {
        let (_root, file) = a_project("bare", "<projects/project_template.os/project> project {\n}\n");
        let nodes = app_api::load_document(on_disk(&file)).expect("open");
        let list = overseer::addressing::find(&nodes, "project/Features").expect("no project/Features");
        assert_eq!(
            list.parameters.get("view"),
            Some(&OverseerValue::String("canvas".into())),
            "the list is not drawn as a canvas"
        );
        assert!(list.children.is_empty(), "a bare tracker has features: {:?}", list.children);
        path_of(&nodes, "project/NewFeature/add");
    });
}

#[test]
fn a_feature_added_waits_unplaced() {
    serialised(|| {
        let (_root, file) = a_project("add", EXAMPLE);
        let nodes = app_api::load_document(on_disk(&file)).expect("open");
        let typed = vec![
            app_api::ValueWrite {
                node_path: path_of(&nodes, "project/NewFeature/handle"),
                value: OverseerValue::String("DARK-MODE".into()),
            },
            app_api::ValueWrite {
                node_path: path_of(&nodes, "project/NewFeature/title"),
                value: OverseerValue::String("A dark theme".into()),
            },
        ];
        let add = path_of(&nodes, "project/NewFeature/add");
        app_api::run_event_at(&file, "p.os", add, "click".into(), "s", typed, Vec::new()).expect("the add was refused");

        let lines = feature_lines(&on_disk(&file), "DARK-MODE");
        assert!(lines.contains(&"- title = \"A dark theme\"".to_string()), "{:?}", lines);
        assert!(lines.iter().any(|l| l.starts_with("- added = ")), "{:?}", lines);
        assert!(!lines.iter().any(|l| l.starts_with("- x = ") || l.starts_with("- y = ")), "placed: {:?}", lines);
    });
}

#[test]
fn picking_a_standing_stamps_when_the_feature_was_touched() {
    serialised(|| {
        let (_root, file) = a_project("standing", EXAMPLE);
        let nodes = app_api::load_document(on_disk(&file)).expect("open");
        let path = path_of(&nodes, "project/Features/[PLUGINS]/standing");
        let value = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String("wanted".into()) };
        app_api::run_event_at(&file, "p.os", path, "change".into(), "s", Vec::new(), vec![value])
            .expect("the pick was refused");

        let lines = feature_lines(&on_disk(&file), "PLUGINS");
        assert!(lines.contains(&"- standing = \"wanted\"".to_string()), "{:?}", lines);
        assert!(lines.iter().any(|l| l.starts_with("- touched_at = \"2026-10-05")), "not stamped: {:?}", lines);
    });
}

#[test]
fn a_card_dragged_writes_both_coordinates_in_one_step() {
    serialised(|| {
        let (root, file) = a_project("drag", EXAMPLE);
        let service = DocumentRoot::new(&root).expect("open the root");
        let nodes = service.open("p.os").expect("open");
        let entry = path_of(&nodes, "project/Features/[SPELLCHECK]");
        let steps = overseer::undo::depth(&root, "p.os");
        service
            .command_for(
                "alice",
                Some("p.os"),
                "set_overseer_entry_fields",
                &json!({
                    "entry_path": entry,
                    "settings": [
                        { "node_path": ["x"], "value": { "Float": 600.0 } },
                        { "node_path": ["y"], "value": { "Float": 120.0 } },
                    ],
                }),
            )
            .expect("the placing was refused");

        let lines = feature_lines(&on_disk(&file), "SPELLCHECK");
        assert!(lines.contains(&"- x = 600".to_string()), "{:?}", lines);
        assert!(lines.contains(&"- y = 120".to_string()), "{:?}", lines);
        assert_eq!(overseer::undo::depth(&root, "p.os"), steps + 1, "a drag was more than one step");
    });
}
