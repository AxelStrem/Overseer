//! A calendar with `of=` is a view: it keeps no days of its own, and draws the days of the
//! calendar it names with its own range and layout. The calendar makes a cell for every day in
//! its own range and in each view's, once, at its own address; a view's own `entry=`, a template
//! made from the days', styles the days in that view alone.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;

const DOC: &str = r##"tab t (label="T", mutable=true) {
    int target = 2000

    div (hidden=true) {
        div Day {
            int total = 0
        }
        <Day> HeatDay (background-color=$(total > ../../target ? "#e5534b" : "#30a14e")) { }
    }

    date selected (mutable="guarded") = "2026-11-05"

    calendar Days (entry=<Day>, span="day", at=$(../selected), layout="day") {
        - 2026-11-01 {
            - total = 2500
        }
        - 2026-11-04 {
            - total = 1500
        }
    }

    div (layout="horizontal") {
        calendar Recent (of="../Days", from=$(date_add_days(../selected, -4)), to=$(../selected), layout="agenda")
        calendar Heatmap (of="Days", entry=<HeatDay>, from="2026-10-26", to="2026-11-08", layout="year")
    }

    int recent = $(Recent.cells().sum(|d| d/total))
    int shown = $(Heatmap.cells().count())
    int first = $(Recent.at("2026-11-01")/total)

    button eat (label="Eat") {
        on click {
            set (path="../Days/2026-11-03/total") = 3000
        }
    }

    button back (label="Back") {
        on click {
            set (path="../selected") = $(date_add_days(../selected, -1))
        }
    }
}
"##;

/// Saving consults a process-wide registry of parsed sources, and the view state is
/// process-wide too.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn value_at(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let node = overseer::addressing::find(nodes, address)?;
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

fn marked(nodes: &[OverseerNode], calendar: &str, mark: &str) -> Vec<String> {
    let calendar = overseer::addressing::find(nodes, calendar).expect("the calendar");
    calendar
        .children
        .iter()
        .filter(|c| matches!(c.parameters.get(mark), Some(OverseerValue::Boolean(true))))
        .map(|c| c.name.clone())
        .collect()
}

fn problem(nodes: &[OverseerNode], address: &str) -> Option<String> {
    match overseer::addressing::find(nodes, address)?.parameters.get("_inherit_error") {
        Some(OverseerValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn colour(nodes: &[OverseerNode], day: &str) -> Option<OverseerValue> {
    let cell = overseer::addressing::find(nodes, &format!("t/Days/{}", day))?;
    cell.parameters.get("_computed__view@t/Heatmap@background-color").cloned()
}

#[test]
fn the_calendar_makes_each_day_once_for_itself_and_its_views() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(marked(&nodes, "t/Days", "_in_view"), ["2026-11-05"]);
    assert_eq!(
        marked(&nodes, "t/Days", "_in_view@t/Recent"),
        ["2026-11-01", "2026-11-02", "2026-11-03", "2026-11-04", "2026-11-05"]
    );
    assert_eq!(marked(&nodes, "t/Days", "_in_view@t/Heatmap").len(), 14);
    let days = overseer::addressing::find(&nodes, "t/Days").unwrap();
    assert_eq!(days.children.len(), 14);
    for view in ["t/Recent", "t/Heatmap"] {
        let node = overseer::addressing::find(&nodes, view).unwrap();
        assert!(node.children.is_empty(), "{} holds days", view);
        assert_eq!(node.parameters.get("_view_of"), Some(&OverseerValue::String("t/Days".into())));
        assert_eq!(problem(&nodes, view), None);
    }
    assert_eq!(problem(&nodes, "t/Days"), None);
}

#[test]
fn a_view_saves_nothing_and_the_document_as_it_was_written() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DOC);
}

#[test]
fn a_formula_reads_a_views_days_in_its_range() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(value_at(&nodes, "t/recent"), Some(OverseerValue::Integer(4000)));
    assert_eq!(value_at(&nodes, "t/shown"), Some(OverseerValue::Integer(14)));
    assert_eq!(value_at(&nodes, "t/first"), Some(OverseerValue::Integer(2500)));
}

#[test]
fn a_views_own_entry_styles_the_days_in_that_view_alone() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(colour(&nodes, "2026-11-01"), Some(OverseerValue::String("#e5534b".into())));
    assert_eq!(colour(&nodes, "2026-11-04"), Some(OverseerValue::String("#30a14e".into())));
    let day = overseer::addressing::find(&nodes, "t/Days/2026-11-01").unwrap();
    assert!(!day.parameters.contains_key("background-color"));
    assert!(!day.parameters.keys().any(|k| k.starts_with("_view@t/Recent")));
}

#[test]
fn an_edit_of_a_day_recolours_it_and_is_written_into_the_calendar() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut changed = std::collections::HashMap::new();
    changed.insert("t/Days/2026-11-03/total".to_string(), OverseerValue::Integer(3000));
    let nodes = app_api::resolve_selective(DOC.to_string(), changed.keys().cloned().collect(), Some(changed))
        .expect("the edit");
    assert_eq!(colour(&nodes, "2026-11-03"), Some(OverseerValue::String("#e5534b".into())));
    assert_eq!(value_at(&nodes, "t/recent"), Some(OverseerValue::Integer(7000)));
    let expected = DOC.replace(
        "        - 2026-11-04 {\n",
        "        - 2026-11-03 {\n            - total = 3000\n        }\n        - 2026-11-04 {\n",
    );
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), expected);
}

#[test]
fn a_views_range_moving_makes_the_calendar_again() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("overseer_calendar_view_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOC).unwrap();
    overseer::viewstate::forget_all();
    let service = DocumentRoot::new(&root).expect("open the root");

    let nodes = service.open_for("someone", "d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, "t/back").expect("the button");
    for _ in 0..8 {
        service
            .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
            .expect("the press was refused");
    }
    let seen = service.open_for("someone", "d.os").expect("open again");
    assert_eq!(marked(&seen, "t/Days", "_in_view"), ["2026-10-28"]);
    assert_eq!(
        marked(&seen, "t/Days", "_in_view@t/Recent"),
        ["2026-10-24", "2026-10-25", "2026-10-26", "2026-10-27", "2026-10-28"]
    );
    assert_eq!(value_at(&seen, "t/recent"), Some(OverseerValue::Integer(0)));
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), DOC, "the press wrote the file");
}

#[test]
fn a_press_writing_a_day_reaches_what_reads_it_through_a_view() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("overseer_calendar_view_eat_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOC).unwrap();
    overseer::viewstate::forget_all();
    let service = DocumentRoot::new(&root).expect("open the root");

    let nodes = service.open_for("someone", "d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, "t/eat").expect("the button");
    let answer = service
        .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .expect("the press was refused");
    let seen = service.open_for("someone", "d.os").expect("open again");
    assert_eq!(value_at(&seen, "t/recent"), Some(OverseerValue::Integer(7000)), "{}", answer.to_string().len());
    assert_eq!(colour(&seen, "2026-11-03"), Some(OverseerValue::String("#e5534b".into())));
    assert!(std::fs::read_to_string(root.join("d.os")).unwrap().contains("- 2026-11-03 {
            - total = 3000
"));
}

#[test]
fn what_is_wrong_with_a_view_is_said_on_it() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let said = |from: &str, to: &str| {
        let doc = DOC.replace(from, to);
        let nodes = app_api::load_document(doc).unwrap();
        problem(&nodes, "t/Heatmap").unwrap_or_default()
    };
    assert!(said("of=\"Days\"", "of=\"target\"").contains("names no calendar"));
    assert!(said("of=\"Days\"", "of=\"Nowhere\"").contains("names no calendar"));
    assert!(said("of=\"Days\"", "of=\"Recent\"").contains("shows another calendar's days itself"));
    assert!(said("entry=<HeatDay>", "entry=<Day>").is_empty());
    assert!(said("entry=<HeatDay>", "entry=int").contains("made from"));
    assert!(said(
        "<Day> HeatDay (background-color=$(total > ../../target ? \"#e5534b\" : \"#30a14e\")) { }",
        "div HeatDay (background-color=\"red\") { int total = 0 }"
    )
    .contains("made from"));
    assert!(said(
        "<Day> HeatDay (background-color=$(total > ../../target ? \"#e5534b\" : \"#30a14e\")) { }",
        "<Day> HeatDay (background-color=\"red\") {\n            int extra = 1\n        }"
    )
    .contains("extra"));
    assert!(said(
        "layout=\"year\")\n",
        "layout=\"year\") {\n            - 2026-11-02 {\n                - total = 1\n            }\n        }\n"
    )
    .contains("holds none of its own"));
}
