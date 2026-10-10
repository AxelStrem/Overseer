//! A closed item can be reopened.
//!
//! Finishing or cancelling an item moves it to History, and until now nothing moved it back: an
//! item closed too early had to be filed again by hand. `reopen`, on every record, puts the item
//! back on the open list at filed with what the record kept, and takes the record out of History.
//! It is hidden while the record's parent is closed too, so a closed item never has an open part -
//! the parent is reopened first, which is always allowed, since all its parts are closed.

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
    let root = std::env::temp_dir().join(format!("overseer_reopen_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, TEMPLATE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-06T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    file.to_string_lossy().to_string()
}

fn opened(file: &str) -> Vec<OverseerNode> {
    app_api::load_document(std::fs::read_to_string(file).unwrap()).expect("open")
}

fn value_at(file: &str, address: &str) -> Option<OverseerValue> {
    let nodes = opened(file);
    let node = overseer::addressing::find(&nodes, address)?;
    node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")).cloned()
}

fn text_at(file: &str, address: &str) -> String {
    match value_at(file, address) {
        Some(OverseerValue::String(s)) => s,
        Some(OverseerValue::Timestamp(s)) => s,
        Some(OverseerValue::Integer(i)) => i.to_string(),
        Some(OverseerValue::Float(f)) => f.to_string(),
        other => panic!("{} reads {:?}", address, other),
    }
}

fn is_there(file: &str, address: &str) -> bool {
    overseer::addressing::find(&opened(file), address).is_some()
}

fn press(file: &str, address: &str) -> app_api::ResolvedUpdate {
    let path = overseer::addressing::name_path(&opened(file), address).unwrap_or_else(|| panic!("nothing at {}", address));
    app_api::run_event_at(file, "p.os", path, "click".into(), "s", Vec::new(), Vec::new()).expect("the press was refused")
}

fn pick(file: &str, handle: &str, stage: &str) {
    let path = overseer::addressing::name_path(&opened(file), &format!("project/Items/[{}]/stage", handle))
        .unwrap_or_else(|| panic!("no stage on {}", handle));
    let value = app_api::ValueWrite { node_path: path.clone(), value: OverseerValue::String(stage.into()) };
    assert!(
        app_api::run_event_at(file, "p.os", path, "change".into(), "s", Vec::new(), vec![value]).expect("the pick").wrote,
        "picking {} on {} wrote nothing",
        stage,
        handle
    );
}

fn reopen_hidden(file: &str, handle: &str) -> bool {
    let nodes = opened(file);
    let button = overseer::addressing::find(&nodes, &format!("project/History/[{}]/reopen", handle))
        .unwrap_or_else(|| panic!("no reopen on {}", handle));
    matches!(
        button.parameters.get("_computed_hidden").or_else(|| button.parameters.get("hidden")),
        Some(OverseerValue::Boolean(true))
    )
}

#[test]
fn a_finished_record_comes_back_at_filed_with_what_it_kept() {
    serialised(|| {
        let file = a_project("finished");
        // roundtrip is finished, under parser, which is open.
        let kept = ["added", "title", "parent", "labels", "points", "commentary"]
            .map(|f| (f, text_at(&file, &format!("project/History/[roundtrip]/{}", f))));
        assert!(press(&file, "project/History/[roundtrip]/reopen").wrote, "reopening wrote nothing");

        assert!(!is_there(&file, "project/History/[roundtrip]"), "the record is still in History");
        assert!(is_there(&file, "project/Items/[roundtrip]"), "the item is not open");
        for (field, was) in kept {
            assert_eq!(text_at(&file, &format!("project/Items/[roundtrip]/{}", field)), was, "{} changed", field);
        }
        assert_eq!(text_at(&file, "project/Items/[roundtrip]/stage"), "filed");
        assert_eq!(text_at(&file, "project/Items/[roundtrip]/complexity"), "unassigned");
    });
}

#[test]
fn the_parent_counts_a_reopened_part_as_open_again() {
    serialised(|| {
        let file = a_project("rollup");
        // parser holds brace, open, and roundtrip, finished.
        assert_eq!(text_at(&file, "project/Items/[parser]/open_kids"), "1");
        let done = text_at(&file, "project/Items/[parser]/done");
        press(&file, "project/History/[roundtrip]/reopen");
        assert_eq!(text_at(&file, "project/Items/[parser]/open_kids"), "2");
        assert_eq!(text_at(&file, "project/Items/[parser]/kids"), "2");
        assert_ne!(text_at(&file, "project/Items/[parser]/done"), done, "done did not move");
    });
}

#[test]
fn a_record_under_a_closed_parent_cannot_be_reopened() {
    serialised(|| {
        let file = a_project("closed_parent");
        // Close parser's parts, then parser: roundtrip's parent is now in History too.
        pick(&file, "brace", "finished");
        pick(&file, "parser", "finished");
        assert!(reopen_hidden(&file, "roundtrip"), "a part can be reopened under a closed parent");
        assert!(!reopen_hidden(&file, "parser"), "the parent cannot be reopened");

        press(&file, "project/History/[parser]/reopen");
        assert!(!reopen_hidden(&file, "roundtrip"), "the part stays hidden once the parent is open");
    });
}

#[test]
fn a_cancelled_record_reopens_the_same_way() {
    serialised(|| {
        let file = a_project("cancelled");
        pick(&file, "brace", "cancelled");
        assert_eq!(text_at(&file, "project/History/[brace]/status"), "cancelled");
        press(&file, "project/History/[brace]/reopen");

        assert!(!is_there(&file, "project/History/[brace]"));
        assert_eq!(text_at(&file, "project/Items/[brace]/stage"), "filed");
        assert_eq!(text_at(&file, "project/Items/[brace]/parent"), "parser");
        assert!(text_at(&file, "project/Items/[brace]/commentary").starts_with("only when"), "the note was lost");
    });
}
