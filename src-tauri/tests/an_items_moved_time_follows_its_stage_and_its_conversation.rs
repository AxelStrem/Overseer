//! An item's `moved` time says when anything last happened to it: it was filed, its stage changed,
//! a comment was added, edited or removed, or one of its parts closed.
//!
//! The stage and the comment buttons stamp a stored `touched_at` on the item, since a stage change
//! or a removed comment leaves no time of its own behind; a comment's own `at` counts as well, so a
//! comment written into the file by a script moves the item without pressing anything.

use chrono::{DateTime, Utc};
use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const EXAMPLE: &str = include_str!("../../examples/projects/project.os");

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run as the server and the app run a document: under settings - here the default, there being
/// no file - which is also where `me` is declared, that the template's "add comment" signs with.
fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let settings = std::env::temp_dir().join(format!("overseer_moved_time_settings_{}.os", std::process::id()));
    let out = overseer::environment::with_file(settings, body);
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text).unwrap_or_else(|_| panic!("not a time: {}", text)).with_timezone(&Utc)
}

fn clock(text: &str) {
    FormulaEvaluator::set_time_override(Some(at(text)));
}

fn a_project(tag: &str, text: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_project_moved_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, text).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    clock("2026-10-01T09:00:00Z");
    file.to_string_lossy().to_string()
}

fn opened(file: &str) -> Vec<OverseerNode> {
    app_api::load_document(std::fs::read_to_string(file).unwrap()).expect("open")
}

fn names_of(file: &str, address: &str) -> Vec<String> {
    overseer::addressing::name_path(&opened(file), address).unwrap_or_else(|| panic!("nothing at {}", address))
}

fn event(file: &str, address: &str, kind: &str, writing: Vec<app_api::ValueWrite>) -> app_api::ResolvedUpdate {
    app_api::run_event_at(file, "p.os", names_of(file, address), kind.into(), "s", Vec::new(), writing)
        .expect("the event was refused")
}

fn moved(file: &str, handle: &str) -> DateTime<Utc> {
    let nodes = opened(file);
    let address = format!("project/Items/[{}]/moved_at", handle);
    let node = overseer::addressing::find(&nodes, &address).unwrap_or_else(|| panic!("nothing at {}", address));
    match node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")) {
        Some(OverseerValue::String(s)) | Some(OverseerValue::Timestamp(s)) => at(s),
        other => panic!("{} moved at {:?}", handle, other),
    }
}

/// The address of the first comment's field of this name on an item.
fn first_comment(file: &str, handle: &str, field: &str) -> String {
    let prefix = format!("project/Items/[{}]/Talk/Comments/", handle);
    overseer::addressing::addresses(&opened(file))
        .into_iter()
        .find(|a| a.starts_with(&prefix) && a.ends_with(&format!("/{}", field)))
        .unwrap_or_else(|| panic!("{} has no comment with a {}", handle, field))
}

#[test]
fn an_item_untouched_since_it_was_filed_moved_when_it_was_filed() {
    serialised(|| {
        let file = a_project("filed", EXAMPLE);
        assert_eq!(moved(&file, "rows"), at("2026-09-02T10:00:00+04:00"));
        // brace's one comment was said as it was filed; editor's last part closed after it was.
        assert_eq!(moved(&file, "brace"), at("2026-09-01T09:10:00+04:00"));
        assert_eq!(moved(&file, "editor"), at("2026-09-05T17:20:00+04:00"));
    });
}

#[test]
fn a_stage_change_moves_the_item() {
    serialised(|| {
        let file = a_project("stage", EXAMPLE);
        clock("2026-10-02T08:00:00Z");
        let path = names_of(&file, "project/Items/[rows]/stage");
        let value = app_api::ValueWrite { node_path: path, value: OverseerValue::String("planning".into()) };
        assert!(event(&file, "project/Items/[rows]/stage", "change", vec![value]).wrote);
        assert_eq!(moved(&file, "rows"), at("2026-10-02T08:00:00Z"));
    });
}

#[test]
fn adding_editing_and_removing_a_comment_each_move_the_item() {
    serialised(|| {
        let file = a_project("talk", EXAMPLE);

        clock("2026-10-03T08:00:00Z");
        let answer = event(&file, "project/Items/[rows]/Talk/add", "click", Vec::new());
        assert!(answer.wrote, "adding a comment wrote nothing");
        assert_eq!(moved(&file, "rows"), at("2026-10-03T08:00:00Z"), "after adding");

        // Typed as the page types it: the value written, then the field's change asked for.
        clock("2026-10-04T08:00:00Z");
        let body = first_comment(&file, "rows", "body");
        let typed = app_api::ValueWrite { node_path: names_of(&file, &body), value: OverseerValue::String("seen".into()) };
        app_api::write_values_at(&file, "p.os", vec![typed], "s").expect("the comment was not written");
        assert!(event(&file, &body, "change", Vec::new()).wrote, "editing a comment stamped nothing");
        assert_eq!(moved(&file, "rows"), at("2026-10-04T08:00:00Z"), "after editing");

        clock("2026-10-05T08:00:00Z");
        let drop = first_comment(&file, "rows", "drop");
        assert!(event(&file, &drop, "click", Vec::new()).wrote, "removing a comment wrote nothing");
        assert!(overseer::addressing::addresses(&opened(&file)).iter().all(|a| !a.starts_with("project/Items/[rows]/Talk/Comments/")));
        assert_eq!(moved(&file, "rows"), at("2026-10-05T08:00:00Z"), "after removing");
    });
}

#[test]
fn a_comment_written_into_the_file_moves_the_item() {
    serialised(|| {
        // favicon's comment, as a script adding one later would leave it.
        let later = EXAMPLE.replacen(
            "- at = \"2026-09-04T16:00:00+04:00\"",
            "- at = \"2026-09-20T12:00:00+04:00\"",
            1,
        );
        assert_ne!(later, EXAMPLE, "the example's favicon comment is not where this expects it");
        let file = a_project("written", &later);
        assert_eq!(moved(&file, "favicon"), at("2026-09-20T12:00:00+04:00"));
    });
}

#[test]
fn a_closed_item_keeps_its_stamp_and_reopening_moves_it() {
    serialised(|| {
        let file = a_project("reopen", EXAMPLE);
        clock("2026-10-06T08:00:00Z");
        let path = names_of(&file, "project/Items/[rows]/stage");
        let value = app_api::ValueWrite { node_path: path, value: OverseerValue::String("finished".into()) };
        assert!(event(&file, "project/Items/[rows]/stage", "change", vec![value]).wrote);
        let written = std::fs::read_to_string(&file).unwrap();
        let history = &written[written.find("- History {").unwrap()..];
        let record = &history[history.find("- handle = \"rows\"").expect("no record of rows")..];
        let record = &record[..record.find("\n        }").unwrap_or(record.len())];
        assert!(record.contains("- touched_at = \"2026-10-06T08:00:00"), "the stamp did not go with it:\n{}", record);

        clock("2026-10-07T08:00:00Z");
        assert!(event(&file, "project/History/[rows]/reopen", "click", Vec::new()).wrote);
        assert_eq!(moved(&file, "rows"), at("2026-10-07T08:00:00Z"));
    });
}
