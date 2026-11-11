//! A day that only a `layout="year"` draws is a square coloured by its style. Its own formulas
//! are worked out, the style among them, and nothing under it; a formula reading a field of it
//! works that field out where it reads it. A day picked into a range that draws it in full, or
//! read over the server, is worked out in full.

use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;

const DOC: &str = r##"tab t (label="T", mutable=true) {
    int target = 2000

    div (hidden=true) {
        div Day {
            int total = 0
            int doubled = $(total * 2)
        }
        <Day> HeatDay (background-color=$(doubled > ../../target ? "#e5534b" : "#30a14e")) { }
        int Goal = $(/t/target / 100)
    }

    date selected (mutable="guarded") = "2026-11-05"

    calendar Days (entry=<Day>, span="day", at=$(../selected), layout="day") {
        - 2026-10-27 {
            - total = 1500
        }
        - 2026-11-04 {
            - total = 500
        }
    }

    calendar Recent (of="Days", from=$(date_add_days(../selected, -1)), to=$(../selected), layout="agenda")
    calendar Year (of="Days", entry=<HeatDay>, from="2026-10-26", to="2026-11-08", layout="year")

    calendar Goals (entry=<Goal>, span="day", at=$(../selected))
    calendar GoalYear (of="Goals", from="2026-10-26", to="2026-11-08", layout="year")

    int far = $(Days.at("2026-10-27")/doubled)

    button eat (label="Eat") {
        on click {
            set (path="../Days/2026-10-28/total") = 1200
        }
    }

    button pick (label="Pick") {
        on click {
            set (path="../selected") = "2026-10-27"
        }
    }
}
"##;

/// Saving consults a process-wide registry of parsed sources, and the view state is
/// process-wide too.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_year_squares_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOC).unwrap();
    overseer::app_api::forget_baseline();
    overseer::viewstate::forget_all();
    root
}

fn squares(nodes: &[OverseerNode], calendar: &str) -> Vec<String> {
    let calendar = overseer::addressing::find(nodes, calendar).expect("the calendar");
    calendar
        .children
        .iter()
        .filter(|c| overseer::resolver::drawn_by_style(c))
        .map(|c| c.name.clone())
        .collect()
}

fn worked_out(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    overseer::addressing::find(nodes, address)?.parameters.get("_computed_value").cloned()
}

fn colour(nodes: &[OverseerNode], day: &str) -> Option<OverseerValue> {
    let cell = overseer::addressing::find(nodes, &format!("t/Days/{}", day))?;
    cell.parameters.get("_computed__view@t/Year@background-color").cloned()
}

fn press(service: &DocumentRoot, button: &str) {
    let nodes = service.open_for("someone", "d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, button).expect("the button");
    service
        .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .expect("the press was refused");
}

#[test]
fn a_day_only_a_year_draws_works_out_its_style_and_nothing_under_it() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = overseer::app_api::load_document(DOC.to_string()).unwrap();
    let year = squares(&nodes, "t/Days");
    assert_eq!(year.len(), 12, "{:?}", year);
    assert!(!year.iter().any(|d| d == "2026-11-04" || d == "2026-11-05"), "{:?}", year);

    assert_eq!(colour(&nodes, "2026-10-27"), Some(OverseerValue::String("#e5534b".into())));
    assert_eq!(colour(&nodes, "2026-10-30"), Some(OverseerValue::String("#30a14e".into())));
    assert_eq!(worked_out(&nodes, "t/Days/2026-10-27/doubled"), None);
    assert_eq!(worked_out(&nodes, "t/Days/2026-11-04/doubled"), Some(OverseerValue::Integer(1000)));
}

#[test]
fn a_formula_reading_a_square_gets_what_its_field_works_out_to() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = overseer::app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(worked_out(&nodes, "t/far"), Some(OverseerValue::Integer(3000)));
}

#[test]
fn a_field_drawn_as_a_square_works_out_its_own_value() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = overseer::app_api::load_document(DOC.to_string()).unwrap();
    assert!(squares(&nodes, "t/Goals").contains(&"2026-10-27".to_string()));
    assert_eq!(worked_out(&nodes, "t/Goals/2026-10-27"), Some(OverseerValue::Integer(20)));
}

#[test]
fn a_day_picked_into_the_calendars_own_range_is_worked_out_in_full() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("pick");
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "t/pick");
    let seen = service.open_for("someone", "d.os").expect("open again");
    assert!(!squares(&seen, "t/Days").contains(&"2026-10-27".to_string()));
    assert!(squares(&seen, "t/Days").contains(&"2026-11-05".to_string()));
    assert_eq!(worked_out(&seen, "t/Days/2026-10-27/doubled"), Some(OverseerValue::Integer(3000)));
}

#[test]
fn a_press_writing_a_square_moves_its_colour() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("eat");
    let service = DocumentRoot::new(&root).expect("open the root");
    let before = service.open_for("someone", "d.os").expect("open");
    assert_eq!(colour(&before, "2026-10-28"), Some(OverseerValue::String("#30a14e".into())));
    press(&service, "t/eat");
    let seen = service.open_for("someone", "d.os").expect("open again");
    assert!(squares(&seen, "t/Days").contains(&"2026-10-28".to_string()));
    assert_eq!(colour(&seen, "2026-10-28"), Some(OverseerValue::String("#e5534b".into())));
}

#[test]
fn the_server_reads_a_square_worked_out_in_full() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("read");
    let service = DocumentRoot::new(&root).expect("open the root");
    service.open("d.os").expect("open");
    let read = service.read_at("d.os", "t/Days/2026-10-27/doubled").expect("the read was refused");
    assert_eq!(read.node.parameters.get("_computed_value"), Some(&OverseerValue::Integer(3000)));
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), DOC, "a read wrote the file");
}
