//! An array holds `length` elements, each made from its `entry=` and named by the grid's column
//! letters alone (`A`, `B`, ... `AA`). It is a grid of one row: what its block writes overrides
//! only the element it names, only that is saved, and an element is reached by its name or by
//! its place from 0 - `Week[2]` in an action, `Week.at(2)` in a formula, `index()` inside one.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::*;

const DOC: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Day {
            int steps = 0
            int place = $(index())
            int before = $(index() > 0 ? Week.at(index() - 1)/steps : 0)
        }
    }

    array Week (entry=<Day>, length=7) {
        - C {
            - steps = 8200
        }
    }

    array Scores (entry=int, length=3) {
        - B = 4
    }

    int total = $(Week.cells().sum(|d| d/steps) + Scores.cells().sum())
    int third = $(Week.at(2)/steps)
    int second_score = $(Scores.at(1))
    int days = $(Week.cells().count())

    button mark (label="mark") {
        on click {
            set (path="../Week[0]/steps") = 5
        }
    }
    button by_name (label="by name") {
        on click {
            set (path="../Week/G/steps") = 6
        }
    }
    button score (label="score") {
        on click {
            set (path="../Scores[2]") = 9
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

fn int(v: Option<OverseerValue>) -> i64 {
    match v {
        Some(OverseerValue::Integer(i)) => i,
        Some(OverseerValue::Float(f)) if f.fract() == 0.0 => f as i64,
        other => panic!("not a whole number: {other:?}"),
    }
}

fn set(nodes: &mut Vec<OverseerNode>, address: &str, value: OverseerValue) {
    let path = overseer::addressing::name_path(nodes, address).expect("a path to the element");
    overseer::actions::ActionExecutor::assign_value(nodes, &format!("/{}", path.join("/")), value)
        .expect("set the value");
}

#[test]
fn every_element_is_made_and_named_by_its_letters() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    let week = overseer::addressing::find(&nodes, "t/Week").expect("the array");
    let names: Vec<&str> = week.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["A", "B", "C", "D", "E", "F", "G"]);
    assert_eq!(int(value_at(&nodes, "t/Week/C/steps")), 8200);
    assert_eq!(int(value_at(&nodes, "t/Week/D/steps")), 0);
    assert_eq!(int(value_at(&nodes, "t/Scores/B")), 4);
    assert_eq!(int(value_at(&nodes, "t/Scores/C")), 0);
}

#[test]
fn an_untouched_array_saves_as_it_was_written() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), DOC);
}

#[test]
fn an_element_set_since_is_written_into_the_block() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut nodes = app_api::load_document(DOC.to_string()).unwrap();
    set(&mut nodes, "t/Week/A/steps", OverseerValue::Integer(5));
    set(&mut nodes, "t/Scores/C", OverseerValue::Integer(9));
    let saved = OverseerFileHandler::serialize_nodes(&nodes).unwrap();
    let expected = DOC
        .replace(
            "        - C {\n",
            "        - A {\n            - steps = 5\n        }\n        - C {\n",
        )
        .replace("        - B = 4\n", "        - B = 4\n        - C = 9\n");
    assert_eq!(saved, expected);
    let reread = app_api::load_document(saved.clone()).unwrap();
    assert_eq!(int(value_at(&reread, "t/Week/A/steps")), 5);
    assert_eq!(int(value_at(&reread, "t/Scores/C")), 9);
    assert_eq!(OverseerFileHandler::serialize_nodes(&reread).unwrap(), saved);
}

#[test]
fn a_formula_reads_elements_by_place() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    assert_eq!(int(value_at(&nodes, "t/total")), 8204);
    assert_eq!(int(value_at(&nodes, "t/third")), 8200);
    assert_eq!(int(value_at(&nodes, "t/second_score")), 4);
    assert_eq!(int(value_at(&nodes, "t/days")), 7);
}

#[test]
fn an_element_knows_its_place_and_reads_the_one_before() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = app_api::load_document(DOC.to_string()).unwrap();
    for (place, letter) in ["A", "B", "C", "D", "E", "F", "G"].iter().enumerate() {
        assert_eq!(int(value_at(&nodes, &format!("t/Week/{letter}/place"))), place as i64);
    }
    assert_eq!(int(value_at(&nodes, "t/Week/D/before")), 8200);
    assert_eq!(int(value_at(&nodes, "t/Week/C/before")), 0);
    assert_eq!(int(value_at(&nodes, "t/Week/A/before")), 0);
}

#[test]
fn a_place_past_the_end_is_an_error() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = DOC.replace("$(Scores.at(1))", "$(Scores.at(3))");
    let nodes = app_api::load_document(doc).unwrap();
    let node = overseer::addressing::find(&nodes, "t/second_score").expect("the field");
    assert!(
        !matches!(node.parameters.get("_computed_value"), Some(OverseerValue::Integer(_))),
        "{:?}",
        node.parameters.get("_computed_value")
    );
}

#[test]
fn index_outside_an_array_is_an_error() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = DOC.replace("$(Week.cells().count())", "$(index())");
    let nodes = app_api::load_document(doc).unwrap();
    let node = overseer::addressing::find(&nodes, "t/days").expect("the field");
    assert!(
        !matches!(node.parameters.get("_computed_value"), Some(OverseerValue::Integer(_))),
        "{:?}",
        node.parameters.get("_computed_value")
    );
}

#[test]
fn an_element_written_past_the_end_is_kept_and_said() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = DOC.replace("        - B = 4\n", "        - B = 4\n        - D = 1\n");
    let nodes = app_api::load_document(doc.clone()).unwrap();
    let scores = overseer::addressing::find(&nodes, "t/Scores").expect("the array");
    let why = scores.parameters.get("_inherit_error").expect("a problem said");
    assert!(matches!(why, OverseerValue::String(s) if s.contains("element D")), "{:?}", why);
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), doc);
}

#[test]
fn an_array_without_a_length_is_said() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let doc = DOC.replace("length=3", "length=0");
    let nodes = app_api::load_document(doc).unwrap();
    let scores = overseer::addressing::find(&nodes, "t/Scores").expect("the array");
    let why = scores.parameters.get("_inherit_error").expect("a problem said");
    assert!(matches!(why, OverseerValue::String(s) if s.contains("length=")), "{:?}", why);
}

fn press(button: &str, tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_array_{}_{}", tag, std::process::id()));
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
fn a_press_names_an_element_by_its_place() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/mark", "place");
    let expected = DOC.replace(
        "        - C {\n",
        "        - A {\n            - steps = 5\n        }\n        - C {\n",
    );
    assert_eq!(written, expected);
}

#[test]
fn a_press_names_an_element_by_its_name() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/by_name", "name");
    let expected = DOC.replace(
        "            - steps = 8200\n        }\n",
        "            - steps = 8200\n        }\n        - G {\n            - steps = 6\n        }\n",
    );
    assert_eq!(written, expected);
}

#[test]
fn a_press_sets_an_element_of_plain_values() {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let written = press("t/score", "plain");
    assert_eq!(written, DOC.replace("        - B = 4\n", "        - B = 4\n        - C = 9\n"));
}

#[test]
fn an_element_name_and_its_place_agree() {
    use overseer::addressing::{element_name, element_place};
    assert_eq!(element_name(0), "A");
    assert_eq!(element_name(25), "Z");
    assert_eq!(element_name(26), "AA");
    assert_eq!(element_name(702), "AAA");
    for place in 0..800 {
        assert_eq!(element_place(&element_name(place)), Some(place));
    }
    assert_eq!(element_place(""), None);
    assert_eq!(element_place("a"), None);
    assert_eq!(element_place("A1"), None);
}
