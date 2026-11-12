//! A path that goes on after `at(...)` can end in a method of the list it names -
//! `Days.at(day)/meals.map(|x| x/kcal).sum()` - in a document formula and in a press, the same as
//! the `Days/day/meals...` form, and a change to an entry of that list reaches the formula.

use overseer::server::DocumentRoot;
use overseer::types::*;
use serde_json::json;

const DOC: &str = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Meal {
            string food = ""
            int grams = 100
            int kcal = $(grams * 2)
        }
        div Day {
            int logged = 0
            int eaten = $(meals.map(|x| x/kcal).sum())
            list meals (entry=<Meal>)
        }
        <Day> HeatDay (background-color=$(logged > 1000 ? "#e5534b" : "#30a14e")) { }
    }

    calendar Days (entry=<Day>, span="day", at="2026-11-05", layout="day") {
        - 2026-10-27 {
            - logged = 1500
            - meals {
                - {
                    - food = "porridge"
                    - grams = 200
                }
                - {
                    - food = "soup"
                }
            }
        }
        - 2026-11-05 {
            - meals {
                - {
                    - food = "rice"
                    - grams = 300
                }
            }
        }
    }

    calendar Year (of="Days", entry=<HeatDay>, from="2026-10-26", to="2026-11-08", layout="year")

    int copied = 0

    button copy (label="Copy") {
        on click {
            set (path="../copied") = $(../Days.at("2026-11-05")/meals.map(|x| x/kcal).sum())
        }
    }
}
"##;

/// Saving consults a process-wide registry of parsed sources, and the view state is
/// process-wide too.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str, doc: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_path_after_at_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), doc).unwrap();
    overseer::app_api::forget_baseline();
    overseer::viewstate::forget_all();
    root
}

fn with(line: &str) -> String {
    DOC.replace("    int copied = 0\n", &format!("    int copied = 0\n    {}\n", line))
}

fn worked_out(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    overseer::addressing::find(nodes, address)?.parameters.get("_computed_value").cloned()
}

fn far(formula: &str) -> Option<OverseerValue> {
    let nodes = overseer::app_api::load_document(with(&format!("int far = $({})", formula))).unwrap();
    worked_out(&nodes, "t/far")
}

#[test]
fn a_list_method_after_at_reads_the_days_list() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(far(r#"Days.at("2026-11-05")/meals.map(|x| x/kcal).sum()"#), Some(OverseerValue::Integer(600)));
    assert_eq!(far(r#"Days/2026-11-05/meals.map(|x| x/kcal).sum()"#), Some(OverseerValue::Integer(600)));
    assert_eq!(far(r#"Days.at("2026-11-05")/meals.count()"#), Some(OverseerValue::Integer(1)));
}

#[test]
fn a_day_out_of_view_is_read_through_at_as_well() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(far(r#"Days.at("2026-10-27")/meals.map(|x| x/kcal).sum()"#), Some(OverseerValue::Integer(600)));
}

#[test]
fn a_day_nobody_wrote_has_an_empty_list() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(far(r#"Days.at("2026-11-02")/meals.map(|x| x/kcal).sum()"#), Some(OverseerValue::Integer(0)));
}

#[test]
fn a_further_step_after_the_method_is_followed() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(far(r#"Days.at("2026-10-27")/meals.first()/kcal"#), Some(OverseerValue::Integer(400)));
    assert_eq!(far(r#"Days.at("2026-10-27")/meals.filter(|x| x/food == "soup").first()/kcal"#), Some(OverseerValue::Integer(200)));
}

#[test]
fn a_slash_with_spaces_is_still_a_division() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(far(r#"Days.at("2026-11-05")/eaten / 2"#), Some(OverseerValue::Integer(300)));
}

#[test]
fn a_press_reading_a_list_after_at_writes_what_it_works_out_to() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("copy", DOC);
    let service = DocumentRoot::new(&root).expect("open the root");
    let nodes = service.open_for("someone", "d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, "t/copy").expect("the button");
    service
        .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .expect("the press was refused");
    let written = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(written, DOC.replace("    int copied = 0\n", "    int copied = 600\n"));
}

#[test]
fn a_change_to_an_entry_reaches_the_formula() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = with(r#"int far = $(Days.at("2026-11-05")/meals.map(|x| x/kcal).sum())"#);
    let root = a_root("grams", &doc);
    let service = DocumentRoot::new(&root).expect("open the root");
    service.open("d.os").expect("open");
    service
        .set_at("d.os", "t/Days/2026-11-05/meals/Meal__1/grams", OverseerValue::Integer(50))
        .expect("the write was refused");
    let far = service.read_at("d.os", "t/far").expect("the read was refused");
    assert_eq!(far.node.parameters.get("_computed_value"), Some(&OverseerValue::Integer(100)));
}
