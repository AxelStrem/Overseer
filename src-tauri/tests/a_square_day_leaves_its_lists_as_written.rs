//! A day that only a `layout="year"` draws leaves its lists as it wrote them: no entry is made
//! from its template, and nothing walks into them. A formula, a press or the server reaching into
//! one gets the day made whole, and a write into one, the file it saves and an undo of it lose
//! nothing.

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
            int twice = $(logged * 2)
            list meals (entry=<Meal>)
        }
        <Day> HeatDay (background-color=$(logged > 1000 ? "#e5534b" : "#30a14e")) { }
        <Day> MealDay (background-color=$(eaten > 500 ? "#e5534b" : "#30a14e")) { }
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
        - 2026-10-29 {
            - meals {
                - {
                    - food = "bread"
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

    button eat (label="Eat") {
        on click {
            set (path="../Days/2026-11-05/logged") = 600
        }
    }

    button copy (label="Copy") {
        on click {
            set (path="../copied") = $(../Days/2026-10-27/meals.map(|x| x/kcal).sum())
        }
    }
}
"##;

/// Saving consults a process-wide registry of parsed sources, and the view state is
/// process-wide too.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_root(tag: &str, doc: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_square_lists_{}_{}", tag, std::process::id()));
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

fn colour(nodes: &[OverseerNode], view: &str, day: &str) -> Option<OverseerValue> {
    let cell = overseer::addressing::find(nodes, &format!("t/Days/{}", day))?;
    cell.parameters.get(&format!("_computed__view@t/{}@background-color", view)).cloned()
}

fn press(service: &DocumentRoot, button: &str) {
    let nodes = service.open_for("someone", "d.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, button).expect("the button");
    service
        .command_for("someone", Some("d.os"), "run_overseer_event", &json!({ "node_path": path, "event_name": "click" }))
        .expect("the press was refused");
}

#[test]
fn a_squares_list_is_left_as_it_was_written() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = overseer::app_api::load_document(DOC.to_string()).unwrap();
    let square = overseer::addressing::find(&nodes, "t/Days/2026-10-27").expect("the square");
    assert!(overseer::resolver::drawn_by_style(square));
    let meals = overseer::addressing::find(&nodes, "t/Days/2026-10-27/meals").expect("the square's list");
    assert!(overseer::resolver::not_worked_out(meals));
    assert_eq!(meals.children.len(), 2);
    assert!(
        meals.children.iter().all(|meal| !meal.children.iter().any(|f| f.name == "kcal")),
        "an entry of a square's list was made from its template: {:?}",
        meals.children
    );
    assert_eq!(colour(&nodes, "Year", "2026-10-27"), Some(OverseerValue::String("#e5534b".into())));
    assert_eq!(colour(&nodes, "Year", "2026-10-29"), Some(OverseerValue::String("#30a14e".into())));

    // The day drawn in full has its list made, and its formulas worked out.
    assert!(!overseer::resolver::drawn_by_style(overseer::addressing::find(&nodes, "t/Days/2026-11-05").unwrap()));
    assert_eq!(worked_out(&nodes, "t/Days/2026-11-05/meals/Meal__1/kcal"), Some(OverseerValue::Integer(600)));
    assert_eq!(worked_out(&nodes, "t/Days/2026-11-05/eaten"), Some(OverseerValue::Integer(600)));
}

#[test]
fn a_formula_reading_a_squares_list_gets_the_day_made_whole() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = with(r#"int far = $(Days.at("2026-10-27")/eaten)"#);
    let nodes = overseer::app_api::load_document(doc).unwrap();
    assert_eq!(worked_out(&nodes, "t/far"), Some(OverseerValue::Integer(600)));
    // The day it reached, and only that one, is worked out in full.
    assert!(!overseer::resolver::drawn_by_style(overseer::addressing::find(&nodes, "t/Days/2026-10-27").unwrap()));
    assert!(overseer::resolver::drawn_by_style(overseer::addressing::find(&nodes, "t/Days/2026-10-29").unwrap()));
    assert_eq!(colour(&nodes, "Year", "2026-10-27"), Some(OverseerValue::String("#e5534b".into())));
}

#[test]
fn a_style_reading_its_days_list_colours_by_it() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = with(r#"calendar Meals (of="Days", entry=<MealDay>, from="2026-10-26", to="2026-11-08", layout="year")"#);
    let nodes = overseer::app_api::load_document(doc).unwrap();
    assert_eq!(colour(&nodes, "Meals", "2026-10-27"), Some(OverseerValue::String("#e5534b".into())));
    assert_eq!(colour(&nodes, "Meals", "2026-10-29"), Some(OverseerValue::String("#30a14e".into())));
    assert_eq!(colour(&nodes, "Meals", "2026-10-30"), Some(OverseerValue::String("#30a14e".into())));
}

#[test]
fn a_press_reading_a_squares_list_gets_what_it_works_out_to() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("copy", DOC);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "t/copy");
    let written = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(written, DOC.replace("    int copied = 0\n", "    int copied = 600\n"));
}

#[test]
fn a_press_reading_a_squares_field_through_at_gets_what_it_works_out_to() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = DOC.replace(
        "$(../Days/2026-10-27/meals.map(|x| x/kcal).sum())",
        r#"$(../Days.at("2026-10-27")/twice)"#,
    );
    let root = a_root("twice", &doc);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "t/copy");
    let written = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(written, doc.replace("    int copied = 0
", "    int copied = 3000
"));
    let nodes = service.open_for("someone", "d.os").expect("open");
    assert!(overseer::resolver::drawn_by_style(overseer::addressing::find(&nodes, "t/Days/2026-10-27").unwrap()));
    assert!(overseer::resolver::drawn_by_style(overseer::addressing::find(&nodes, "t/Days/2026-10-29").unwrap()));
}

#[test]
fn a_press_elsewhere_saves_the_squares_lists_as_they_were_written() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("eat", DOC);
    let service = DocumentRoot::new(&root).expect("open the root");
    press(&service, "t/eat");
    let written = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(
        written,
        DOC.replace("        - 2026-11-05 {\n", "        - 2026-11-05 {\n            - logged = 600\n")
    );
}

#[test]
fn an_entry_appended_to_a_squares_list_lands_and_is_taken_back() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = a_root("append", DOC);
    let service = DocumentRoot::new(&root).expect("open the root");
    service.open("d.os").expect("open");
    let fields = std::collections::HashMap::from([
        ("food".to_string(), OverseerValue::String("apple".into())),
        ("grams".to_string(), OverseerValue::Integer(50)),
    ]);
    service.append_at("d.os", "t/Days/2026-10-29/meals", &fields).expect("the append was refused");

    let written = std::fs::read_to_string(root.join("d.os")).unwrap();
    let bread = "                    - food = \"bread\"\n                }\n";
    assert_eq!(
        written,
        DOC.replace(
            bread,
            &format!("{}                - {{\n                    - food = \"apple\"\n                    - grams = 50\n                }}\n", bread)
        )
    );
    let eaten = service.read_at("d.os", "t/Days/2026-10-29/eaten").expect("the read was refused");
    assert_eq!(eaten.node.parameters.get("_computed_value"), Some(&OverseerValue::Integer(300)));

    service.undo("d.os").expect("the undo was refused");
    assert_eq!(std::fs::read_to_string(root.join("d.os")).unwrap(), DOC);
}
