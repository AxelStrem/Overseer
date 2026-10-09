//! An item carries workflow flags beside its tags.
//!
//! A project's tags are its own - what kind of work an item is - while its flags are the same in
//! every project, like the stages: manual, auto, first, asked, discuss. They say how an item is to
//! be worked rather than what it is - who it waits on, whether an agent may take it - so they
//! stay with it wherever it goes: onto the record when it is closed, and back when it is reopened.
//! Checked against the template every project is a copy of.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const TEMPLATE: &str = include_str!("../../examples/projects/project_template.os");

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

/// The template in a file of its own.
fn a_project(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_flags_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, TEMPLATE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-08T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    file.to_string_lossy().to_string()
}

fn on_disk(file: &str) -> String {
    std::fs::read_to_string(file).unwrap()
}

fn opened(file: &str) -> Vec<OverseerNode> {
    app_api::load_document(on_disk(file)).expect("open")
}

fn text_at(file: &str, address: &str) -> String {
    let nodes = opened(file);
    let node = overseer::addressing::find(&nodes, address).unwrap_or_else(|| panic!("nothing at {}", address));
    match node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")) {
        Some(OverseerValue::String(s)) => s.clone(),
        other => panic!("{} reads {:?}", address, other),
    }
}

/// The lines of one entry, by its handle: an entry's fields hold no braces, so the first line
/// that is only one closes it.
fn entry_lines(text: &str, handle: &str) -> Vec<String> {
    let at = text.find(&format!("- handle = \"{}\"", handle)).expect(handle);
    let open = text[..at].rfind("- {").expect("the entry's opening");
    text[open + 3..]
        .lines()
        .map(|l| l.trim().to_string())
        .take_while(|l| l != "}")
        .filter(|l| !l.is_empty())
        .collect()
}

/// Write a field of an open item, as the page does when a chip is picked.
fn set(file: &str, handle: &str, field: &str, value: &str) {
    let path = overseer::addressing::name_path(&opened(file), &format!("project/Items/[{}]/{}", handle, field))
        .unwrap_or_else(|| panic!("no {} on {}", field, handle));
    let write = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String(value.into()) };
    app_api::run_event_at(file, "p.os", path, "change".into(), "s", Vec::new(), vec![write])
        .expect("the change was refused");
}

fn press(file: &str, address: &str) {
    let path = overseer::addressing::name_path(&opened(file), address).unwrap_or_else(|| panic!("nothing at {}", address));
    let update = app_api::run_event_at(file, "p.os", path, "click".into(), "s", Vec::new(), Vec::new())
        .expect("the press was refused");
    assert!(update.wrote, "pressing {} wrote nothing", address);
}

#[test]
fn every_project_offers_the_same_flags_in_order() {
    let nodes = app_api::load_document(TEMPLATE.to_string()).expect("open");
    let flags = overseer::addressing::find(&nodes, "project/Flags").expect("a Flags list");
    let tags: Vec<String> = flags
        .children
        .iter()
        .filter_map(|e| e.children.iter().find(|c| c.name == "tag"))
        .filter_map(|c| match c.parameters.get("value") {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tags, ["manual", "auto", "first", "asked", "discuss"]);
}

#[test]
fn an_item_with_no_flags_reads_none() {
    serialised(|| {
        let file = a_project("none");
        assert_eq!(text_at(&file, "project/Items/[docs]/flags"), "");
    });
}

#[test]
fn flags_are_written_after_the_tags() {
    serialised(|| {
        let file = a_project("order");
        set(&file, "docs", "flags", "first, asked");
        let lines = entry_lines(&on_disk(&file), "docs");
        let at = |field: &str| lines.iter().position(|l| l.starts_with(&format!("- {} = ", field)));
        assert_eq!(lines.iter().find(|l| l.starts_with("- flags = ")).map(String::as_str), Some("- flags = \"first, asked\""));
        assert!(at("labels") < at("flags") && at("flags") < at("points"), "out of the template's order: {:?}", lines);
    });
}

#[test]
fn closing_an_item_keeps_its_flags_and_reopening_brings_them_back() {
    serialised(|| {
        let file = a_project("closed");
        set(&file, "docs", "flags", "first, manual");
        set(&file, "docs", "stage", "finished");
        assert_eq!(text_at(&file, "project/History/[docs]/flags"), "first, manual", "the record lost its flags");

        press(&file, "project/History/[docs]/reopen");
        assert_eq!(text_at(&file, "project/Items/[docs]/flags"), "first, manual", "reopening lost the flags");
        assert_eq!(text_at(&file, "project/Items/[docs]/stage"), "filed");
    });
}
