//! An `append` or `prepend` whose entry differs from the template in nothing still writes an
//! entry: `- { }`, which reads back as one entry of the template's values.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r##"div (hidden=true) {
    div Share (layout="horizontal") {
        string document (label="Document") = ""
        string can (label="Can") = "read"
    }
}

tab sharing (label="Sharing", mutable=true) {
    list Shares (entry=<Share>, layout="vertical") { }

    list More (entry=<Share>) {
        - {
            - document = "tasks.os"
        }
    }

    button add {
        on click {
            append (list="../Shares")
        }
    }

    button more {
        on click {
            append (list="../More")
        }
    }

    button first {
        on click {
            prepend (list="../More")
        }
    }
}
"##;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_document(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_emptyappend_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("s.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    FormulaEvaluator::set_time_override(None);
    file.to_string_lossy().to_string()
}

fn on_disk(file: &str) -> String {
    std::fs::read_to_string(file).unwrap()
}

fn press(file: &str, button: &str) {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("sharing/{}", button))
        .unwrap_or_else(|| panic!("no button {}", button));
    app_api::run_event_at(file, "s.os", path, "click".into(), "s", Vec::new(), Vec::new())
        .unwrap_or_else(|e| panic!("press {}: {:?}", button, e));
}

fn documents(file: &str, list: &str) -> Vec<String> {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    let list = overseer::addressing::find(&nodes, list).unwrap_or_else(|| panic!("no {}", list));
    list.children
        .iter()
        .map(|entry| {
            let field = entry.children.iter().find(|c| c.name == "document").expect("document");
            match field.parameters.get("_computed_value").or_else(|| field.parameters.get("value")) {
                Some(OverseerValue::String(s)) => s.clone(),
                other => panic!("reads {:?}", other),
            }
        })
        .collect()
}

fn saved_again(file: &str) -> String {
    let nodes = app_api::load_document(on_disk(file)).expect("open");
    overseer::OverseerFileHandler::serialize_nodes(&nodes).expect("save")
}

#[test]
fn an_append_onto_an_empty_list_writes_an_empty_entry() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let file = a_document("empty");
    press(&file, "add");
    let text = on_disk(&file);
    assert!(
        text.contains("    list Shares (entry=<Share>, layout=\"vertical\") {\n        - { }\n    }\n"),
        "{}",
        text
    );
    assert_eq!(documents(&file, "sharing/Shares"), vec![String::new()]);
    assert_eq!(saved_again(&file), text);
}

#[test]
fn an_append_onto_a_list_with_entries_adds_one_after_them() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let file = a_document("after");
    press(&file, "more");
    let text = on_disk(&file);
    assert!(
        text.contains("        - {\n            - document = \"tasks.os\"\n        }\n        - { }\n    }\n"),
        "{}",
        text
    );
    assert_eq!(documents(&file, "sharing/More"), vec!["tasks.os".to_string(), String::new()]);
    assert_eq!(saved_again(&file), text);
}

#[test]
fn a_prepend_puts_the_empty_entry_first() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let file = a_document("first");
    press(&file, "first");
    let text = on_disk(&file);
    assert!(
        text.contains("    list More (entry=<Share>) {\n        - { }\n        - {\n            - document = \"tasks.os\"\n"),
        "{}",
        text
    );
    assert_eq!(documents(&file, "sharing/More"), vec![String::new(), "tasks.os".to_string()]);
    assert_eq!(saved_again(&file), text);
}
