//! A calendar holds a cell for every day, each made from its `entry=` and named by its date
//! (`2026-11-04`). Its range - `from=` and `to=`, or a `span=` around `at=` - is only which days
//! are in view: a day written outside it is kept, with nothing said. Only what the block wrote is
//! saved, and a cell set since is written into it, among the others in date order.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;

const DOC: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Day {
            int commits = 0
            string note = ""
            date day = $(date())
            int before = $(Days.at(date_add_days(date(), -1))/commits)
        }
    }

    calendar Days (entry=<Day>, from="2026-11-02", to="2026-11-08") {
        - 2026-10-20 {
            - commits = 2
        }
        - 2026-11-04 {
            - commits = 5
        }
    }

    calendar Steps (entry=int, span="month", at="2026-02-10") {
        - 2026-02-03 = 4000
    }

    int total = $(Days.cells().sum(|d| d/commits))
    int shown = $(Days.cells().count())
    int october = $(Days.at("2026-10-20")/commits)
    int never = $(Days.at("2020-01-01")/commits)
    int february = $(Steps.cells().count())
    int third = $(Steps.at("2026-02-03"))
    int unwritten = $(Steps.at("2025-01-01"))

    button mark (label="mark") {
        on click {
            set (path="../Days/2026-11-06/commits") = 3
        }
    }
    button old (label="old") {
        on click {
            set (path="../Days[2026-01-01]/note") = "long ago"
        }
    }
    button first (label="first") {
        on click {
            set (path="../Days[$(date_add_days(../Days/2026-11-04/day, -2))]/commits") = 1
        }
    }
}
"#;

/// Saving consults a process-wide registry of parsed sources, so two of these running at once
/// see each other's document.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn value_at(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let node = overseer::addressing::find(nodes, address)?;
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

fn names(nodes: &[OverseerNode], address: &str) -> Vec<String> {
    let calendar = overseer::addressing::find(nodes, address).expect("the calendar");
    calendar.children.iter().map(|c| c.name.clone()).collect()
}

fn in_view(nodes: &[OverseerNode], address: &str) -> Vec<String> {
    let calendar = overseer::addressing::find(nodes, address).expect("the calendar");
    calendar
        .children
        .iter()
        .filter(|c| matches!(c.parameters.get("_in_view"), Some(OverseerValue::Boolean(true))))
        .map(|c| c.name.clone())
        .collect()
}

fn press(button: &str, tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_calendar_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOC).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let path = file.to_string_lossy().to_string();
    let nodes = app_api::load_document(DOC.to_string()).expect("open");
    let node_path = overseer::addressing::name_path(&nodes, button).expect("the button");
    app_api::run_event_at(&path, "d.os", node_path, "click".into(), "s", Vec::new(), Vec::new())
        .expect("the press was refused");
    std::fs::read_to_string(&file).unwrap()
}

#[test]
fn every_day_in_view_is_made_and_named_by_its_date() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    let week = [
        "2026-11-02", "2026-11-03", "2026-11-04", "2026-11-05", "2026-11-06", "2026-11-07", "2026-11-08",
    ];
    let mut held = vec!["2026-10-20".to_string()];
    held.extend(week.iter().map(|d| d.to_string()));
    assert_eq!(names(&nodes, "t/Days"), held);
    assert_eq!(in_view(&nodes, "t/Days"), week);
    assert_eq!(value_at(&nodes, "t/Days/2026-11-04/commits"), Some(OverseerValue::Integer(5)));
    assert_eq!(value_at(&nodes, "t/Days/2026-11-05/commits"), Some(OverseerValue::Integer(0)));
    assert_eq!(value_at(&nodes, "t/Steps/2026-02-03"), Some(OverseerValue::Integer(4000)));
    assert_eq!(in_view(&nodes, "t/Steps").len(), 28);
}

#[test]
fn a_day_written_outside_the_range_is_kept_and_nothing_is_said() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    let days = overseer::addressing::find(&nodes, "t/Days").expect("the calendar");
    assert!(!days.parameters.contains_key("_inherit_error"), "{:?}", days.parameters.get("_inherit_error"));
    assert_eq!(value_at(&nodes, "t/october"), Some(OverseerValue::Integer(2)));
}

#[test]
fn a_name_that_is_no_day_is_said() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = DOC.replace("        - 2026-02-03 = 4000\n", "        - 2026-02-03 = 4000\n        - B3 = 1\n");
    let nodes = app_api::load_document(doc.clone()).unwrap();
    let steps = overseer::addressing::find(&nodes, "t/Steps").expect("the calendar");
    let why = steps.parameters.get("_inherit_error").expect("a problem said");
    assert!(matches!(why, OverseerValue::String(s) if s.contains("B3")), "{:?}", why);
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), doc);
}

#[test]
fn an_untouched_calendar_saves_as_it_was_written() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DOC);
}

#[test]
fn a_formula_reads_the_days_in_view_and_any_day_by_its_date() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(value_at(&nodes, "t/total"), Some(OverseerValue::Integer(5)));
    assert_eq!(value_at(&nodes, "t/shown"), Some(OverseerValue::Integer(7)));
    assert_eq!(value_at(&nodes, "t/never"), Some(OverseerValue::Integer(0)));
    assert_eq!(value_at(&nodes, "t/february"), Some(OverseerValue::Integer(28)));
    assert_eq!(value_at(&nodes, "t/third"), Some(OverseerValue::Integer(4000)));
    assert_eq!(value_at(&nodes, "t/unwritten"), Some(OverseerValue::Integer(0)));
}

#[test]
fn a_cell_knows_its_own_day() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(value_at(&nodes, "t/Days/2026-11-06/day"), Some(OverseerValue::Date("2026-11-06".into())));
    assert_eq!(value_at(&nodes, "t/Days/2026-11-05/before"), Some(OverseerValue::Integer(5)));
    assert_eq!(value_at(&nodes, "t/Days/2026-11-04/before"), Some(OverseerValue::Integer(0)));
}

#[test]
fn an_edit_from_the_page_is_written_into_the_block_in_date_order() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut changed = std::collections::HashMap::new();
    changed.insert("t/Days/2026-11-03/commits".to_string(), OverseerValue::Integer(1));
    changed.insert("t/Steps/2026-02-20".to_string(), OverseerValue::Integer(9000));
    let nodes = app_api::resolve_selective(DOC.to_string(), changed.keys().cloned().collect(), Some(changed))
        .expect("the edit");
    assert_eq!(value_at(&nodes, "t/total"), Some(OverseerValue::Integer(6)));
    let expected = DOC
        .replace(
            "        - 2026-11-04 {\n",
            "        - 2026-11-03 {\n            - commits = 1\n        }\n        - 2026-11-04 {\n",
        )
        .replace("        - 2026-02-03 = 4000\n", "        - 2026-02-03 = 4000\n        - 2026-02-20 = 9000\n");
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), expected);
    let reread = app_api::load_document(expected.clone()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&reread).unwrap(), expected);
}

#[test]
fn a_press_names_a_day_by_its_name() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/mark", "name");
    let expected = DOC.replace(
        "            - commits = 5\n        }\n",
        "            - commits = 5\n        }\n        - 2026-11-06 {\n            - commits = 3\n        }\n",
    );
    assert_eq!(written, expected);
}

#[test]
fn a_press_names_a_day_out_of_view_and_it_is_kept() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/old", "old");
    let expected = DOC.replace(
        "        - 2026-10-20 {\n",
        "        - 2026-01-01 {\n            - note = \"long ago\"\n        }\n        - 2026-10-20 {\n",
    );
    assert_eq!(written, expected);
}

#[test]
fn a_press_names_a_day_by_a_formula() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/first", "formula");
    let expected = DOC.replace(
        "        - 2026-11-04 {\n",
        "        - 2026-11-02 {\n            - commits = 1\n        }\n        - 2026-11-04 {\n",
    );
    assert_eq!(written, expected);
}

const STEPPED: &str = r#"tab t (label="T", mutable=true) {
    date showing (mutable="guarded") = "2026-11-05"

    calendar Days (entry=int, from=$(date_add_days(../showing, -2)), to=$(../showing)) {
        - 2026-11-01 = 7
    }

    int total = $(Days.cells().sum())
    string first = $(Days.cells().first())

    button back (label="Back") {
        on click {
            set (path="../showing") = $(date_add_days(../showing, -3))
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn a_range_a_button_steps_moves_for_the_viewer_and_leaves_the_file_alone() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("overseer_calendar_step_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), STEPPED).unwrap();
    overseer::viewstate::forget_all();
    let service = DocumentRoot::new(&root).expect("open the root");

    let nodes = service.open_for("someone", "d.os").expect("open");
    assert_eq!(in_view(&nodes, "t/Days"), ["2026-11-03", "2026-11-04", "2026-11-05"]);
    assert_eq!(value_at(&nodes, "t/total"), Some(OverseerValue::Integer(0)));

    let path = overseer::addressing::name_path(&nodes, "t/back").expect("the button");
    service
        .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .expect("the press was refused");

    let seen = service.open_for("someone", "d.os").expect("open again");
    assert_eq!(in_view(&seen, "t/Days"), ["2026-10-31", "2026-11-01", "2026-11-02"]);
    assert_eq!(value_at(&seen, "t/total"), Some(OverseerValue::Integer(7)));
    let others = service.open_for("someone else", "d.os").expect("open as another");
    assert_eq!(in_view(&others, "t/Days"), ["2026-11-03", "2026-11-04", "2026-11-05"]);
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), STEPPED, "the press wrote the file");
}

#[test]
fn a_value_moving_the_range_makes_the_calendar_again() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut changed = std::collections::HashMap::new();
    changed.insert("t/showing".to_string(), OverseerValue::Date("2026-11-02".into()));
    let nodes = app_api::resolve_selective(STEPPED.to_string(), changed.keys().cloned().collect(), Some(changed))
        .expect("the edit");
    assert_eq!(in_view(&nodes, "t/Days"), ["2026-10-31", "2026-11-01", "2026-11-02"]);
    assert_eq!(value_at(&nodes, "t/total"), Some(OverseerValue::Integer(7)));
}

#[test]
fn a_span_holds_the_day_it_is_at() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = |span: &str| {
        format!(
            "div d {{\n    calendar C (entry=int, span=\"{}\", at=\"2026-11-05\") {{ }}\n}}\n",
            span
        )
    };
    let shown = |span: &str| {
        let nodes = app_api::load_document(doc(span)).unwrap();
        let days = in_view(&nodes, "d/C");
        (days.first().cloned().unwrap_or_default(), days.last().cloned().unwrap_or_default(), days.len())
    };
    assert_eq!(shown("day"), ("2026-11-05".into(), "2026-11-05".into(), 1));
    assert_eq!(shown("week"), ("2026-11-02".into(), "2026-11-08".into(), 7));
    assert_eq!(shown("month"), ("2026-11-01".into(), "2026-11-30".into(), 30));
    assert_eq!(shown("year"), ("2026-01-01".into(), "2026-12-31".into(), 365));
}

#[test]
fn a_date_names_a_node_and_nothing_else() {
    use overseer::addressing::date_of;
    assert!(date_of("2026-11-04").is_some());
    assert!(date_of("2026-02-29").is_none());
    assert!(date_of("2026-1-04").is_none());
    assert!(date_of("A1").is_none());
}
