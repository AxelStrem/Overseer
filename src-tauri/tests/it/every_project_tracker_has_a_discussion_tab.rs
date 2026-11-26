//! Every project tracker has a Discussion tab: a list of topics, inherited from the shipped
//! template without the tracker writing anything for it. The tab is unnamed, inside `project`, so
//! the list is `project/Discussion` beside `project/Items`.
//!
//! Text names a topic with a mark of its own, `<:naming>`, beside `<#editor>` for an item, so the
//! mark says which list a chip reads. Their ids still come from one pool: the add rows of both
//! tabs refuse an id the other list holds.

use crate::common::serial;
use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const EXAMPLE: &str = include_str!("../../../examples/projects/project.os");


fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = serial();
    let out = body();
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

fn a_project(tag: &str, text: &str) -> (std::path::PathBuf, String) {
    let root = std::env::temp_dir().join(format!("overseer_discussion_{}_{}", tag, std::process::id()));
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

/// Types an id and a title into an add row and presses its button.
fn add(file: &str, row: &str, handle: &str, title: &str) -> std::result::Result<(), String> {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let typed = vec![
        app_api::ValueWrite {
            node_path: path_of(&nodes, &format!("project/{}/handle", row)),
            value: OverseerValue::String(handle.into()),
        },
        app_api::ValueWrite {
            node_path: path_of(&nodes, &format!("project/{}/title", row)),
            value: OverseerValue::String(title.into()),
        },
    ];
    let button = path_of(&nodes, &format!("project/{}/add", row));
    app_api::run_event_at(file, "p.os", button, "click".into(), "s", typed, Vec::new())
        .map(|_| ())
        .map_err(|e| format!("{:?}", e))
}

#[test]
fn a_tracker_that_never_names_the_tab_has_it_empty() {
    serialised(|| {
        let (_root, file) = a_project("bare", "<projects/project_template.os/project> project {\n}\n");
        let nodes = app_api::load_document(on_disk(&file)).expect("open");
        let list = overseer::addressing::find(&nodes, "project/Discussion").expect("no project/Discussion");
        assert!(list.children.is_empty(), "a bare tracker has topics: {:?}", list.children);
        path_of(&nodes, "project/NewTopic/add");
    });
}

#[test]
fn a_topic_added_goes_on_top_with_when_and_whose() {
    serialised(|| {
        let (_root, file) = a_project("add", EXAMPLE);
        add(&file, "NewTopic", "release", "When is the beta?").expect("the add was refused");

        let text = on_disk(&file);
        let at = text.find("- handle = \"release\"").expect("no topic written");
        let section = text.find("- Discussion {").expect("no Discussion section");
        assert!(text[section..at].find("- handle = \"naming\"").is_none(), "the new topic is not on top");
        let entry = &text[text[..at].rfind("- {").unwrap()..];
        let entry = &entry[..entry.find('}').unwrap()];
        assert!(entry.contains("- title = \"When is the beta?\""), "{}", entry);
        assert!(entry.contains("- added = \"2026-10-05"), "{}", entry);
        assert!(entry.contains("- touched_at = \"2026-10-05"), "{}", entry);
    });
}

#[test]
fn a_topic_cannot_take_an_items_id_nor_an_item_a_topics() {
    serialised(|| {
        let (_root, file) = a_project("taken", EXAMPLE);
        let before = on_disk(&file);
        assert!(add(&file, "NewTopic", "editor", "x").is_err(), "a topic took an open item's id");
        assert!(add(&file, "NewTopic", "escape", "x").is_err(), "a topic took a closed item's id");
        assert!(add(&file, "NewTopic", "", "x").is_err(), "a topic was added without an id");
        assert!(add(&file, "NewTask", "naming", "x").is_err(), "an item took a topic's id");
        assert_eq!(on_disk(&file), before, "a refused add wrote something");
    });
}

#[test]
fn dropping_a_topic_removes_it() {
    serialised(|| {
        let (_root, file) = a_project("drop", EXAMPLE);
        let nodes = app_api::load_document(on_disk(&file)).expect("open");
        let drop = path_of(&nodes, "project/Discussion/[naming]/drop");
        app_api::run_event_at(&file, "p.os", drop, "click".into(), "s", Vec::new(), Vec::new())
            .expect("the drop was refused");
        let text = on_disk(&file);
        assert!(!text.contains("- handle = \"naming\""), "the topic is still there");
        assert!(text.contains("- handle = \"editor\""), "an item went with it");
    });
}

#[test]
fn text_names_a_topic_by_the_colon_mark_and_an_item_by_the_hash() {
    serialised(|| {
        let (_root, file) = a_project("marks", EXAMPLE);
        let nodes = app_api::load_document(on_disk(&file)).expect("open");
        let project = overseer::addressing::find(&nodes, "project").expect("no project");
        let vocabulary = project
            .children
            .iter()
            .find(|child| overseer::contexts::is_copy(child) && child.name == "vocabulary")
            .expect("the project holds no copy of the vocabulary");
        let said: Vec<String> = vocabulary
            .children
            .iter()
            .map(|element| match element.parameters.get("_computed_value").or_else(|| element.parameters.get("value")) {
                Some(OverseerValue::String(s)) => s.clone(),
                other => format!("{:?}", other),
            })
            .collect();
        assert!(said.iter().any(|s| s == ":/project/Discussion"), "no list for `:` names the topics: {:?}", said);
        for s in said.iter().filter(|s| !s.starts_with(|c: char| "@$:%~^+".contains(c))) {
            assert!(!s.contains("/project/Discussion"), "`#` still names the topics: {}", s);
            assert!(s.contains("/project/Items"), "`#` names no items: {}", s);
        }
    });
}
