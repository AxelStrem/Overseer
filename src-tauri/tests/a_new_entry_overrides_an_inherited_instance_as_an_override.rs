//! A list's template can hold an instance of another template, `<Nutrients> per_100g { }`. An
//! entry that sets a figure inside it says so as an override, `- per_100g { - calories = 45 }`,
//! whether the figure came with the entry when it was appended or was set on it afterwards.
//!
//! A new entry used to repeat the template's reference, `<Nutrients> per_100g { ... }`, in every
//! such entry, while an edit of the same figure wrote the override.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const FOODS: &str = r##"tab food_catalog (label="Foods", mutable=true) {
    div (hidden=true) {
        div Nutrients (layout="horizontal") {
            float calories (label="kcal") = 0
            float protein (label="Protein") = 0
            float fibre (label="Fibre") = 0
        }
        div Food (layout="horizontal") {
            string handle (label="Handle") = "unnamed"
            string name (label="Name") = ""
            <Nutrients> per_100g { }
        }
    }

    list Catalog (entry=<Food>, key="handle") {
        - {
            - handle = "latte"
            - name = "Latte"
            - per_100g {
                - calories = 52
            }
        }
    }
}
"##;

const DAY: &str = r##"tab day (label="Day", mutable=true) {
    div (hidden=true) {
        div Meal {
            string name (label="") = ""
            <foods.os/Nutrients> per_100g (hidden=true) {
                - fibre = 1
            }
            float portions = 1
            float kcal = $(portions * per_100g/calories)
        }
    }

    mount FOODS (hidden=true, lazy=false, mutable=false, source="foods.os/food_catalog/Catalog") { }

    list Meals (entry=<Meal>) {
        - {
            - name = "Soup"
        }
    }
}
"##;

fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_override_form_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

fn number(root: &Path, address: &str) -> f64 {
    let service = DocumentRoot::new(root).unwrap();
    let node = service
        .read_at("d.os", address)
        .unwrap_or_else(|e| panic!("nothing read at {}: {:?}", address, e))
        .node;
    match node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")) {
        Some(OverseerValue::Float(f)) => *f,
        Some(OverseerValue::Integer(n)) => *n as f64,
        other => panic!("{} reads {:?}", address, other),
    }
}

fn on_disk(root: &Path) -> String {
    std::fs::read_to_string(root.join("d.os")).unwrap()
}

/// The document as the resolver works it out from the file, written out again.
fn saved_again(root: &Path) -> String {
    let content = on_disk(root);
    let nodes = overseer::docmgr::manager::DocumentManager::with_document(Some(root.to_path_buf()), || {
        let (_rest, mut nodes) = overseer::parser::parse_document(&content).unwrap();
        overseer::resolver::resolve_document(&mut nodes);
        nodes
    });
    overseer::OverseerFileHandler::serialize_nodes(&nodes).unwrap()
}

fn figures() -> HashMap<String, OverseerValue> {
    let mut fields = HashMap::new();
    fields.insert("per_100g/calories".to_string(), OverseerValue::Float(45.0));
    fields.insert("portions".to_string(), OverseerValue::Float(2.0));
    fields
}

#[test]
fn a_figure_appended_with_the_entry_is_an_override() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("plain", &[("foods.os", FOODS), ("d.os", DAY)]);
    let service = DocumentRoot::new(&root).unwrap();
    service.append_at_with("d.os", "day/Meals", &figures(), None).expect("the append");

    let saved = on_disk(&root);
    assert_eq!(
        saved,
        DAY.replace(
            "            - name = \"Soup\"\n        }\n",
            "            - name = \"Soup\"\n        }\n        - {\n            - per_100g {\n                - calories = 45\n            }\n            - portions = 2\n        }\n"
        ),
        "{}",
        saved
    );
    assert_eq!(number(&root, "day/Meals/Meal__2/per_100g/calories"), 45.0);
    // What the template's own instance wrote is still the template's.
    assert_eq!(number(&root, "day/Meals/Meal__2/per_100g/fibre"), 1.0);
    assert_eq!(number(&root, "day/Meals/Meal__2/kcal"), 90.0);
    assert_eq!(saved_again(&root), saved);
}

#[test]
fn an_entry_made_from_a_catalogue_entry_writes_the_same() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("stacked", &[("foods.os", FOODS), ("d.os", DAY)]);
    let service = DocumentRoot::new(&root).unwrap();
    service
        .append_at_with("d.os", "day/Meals", &figures(), Some("FOODS/Catalog/[latte]"))
        .expect("the append");

    let saved = on_disk(&root);
    assert!(
        saved.contains(
            "        - <FOODS/Catalog/[latte]> {\n            - per_100g {\n                - calories = 45\n            }\n            - portions = 2\n        }\n"
        ),
        "{}",
        saved
    );
    assert!(!saved.contains("Nutrients> per_100g {\n                - calories"), "{}", saved);
    assert_eq!(number(&root, "day/Meals/Meal__2/per_100g/calories"), 45.0);
    assert_eq!(saved_again(&root), saved);
}

#[test]
fn a_figure_set_afterwards_keeps_the_form() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("edit", &[("foods.os", FOODS), ("d.os", DAY)]);
    let service = DocumentRoot::new(&root).unwrap();
    service.append_at_with("d.os", "day/Meals", &figures(), None).expect("the append");
    let appended = on_disk(&root);

    service
        .set_at("d.os", "day/Meals/Meal__2/per_100g/calories", OverseerValue::Float(60.0))
        .unwrap();
    assert_eq!(on_disk(&root), appended.replace("- calories = 45", "- calories = 60"));

    // On an entry that said nothing of the figures, an edit writes what the append would have.
    service
        .set_at("d.os", "day/Meals/Meal__1/per_100g/calories", OverseerValue::Float(30.0))
        .unwrap();
    let saved = on_disk(&root);
    assert!(
        saved.contains("            - name = \"Soup\"\n            - per_100g {\n                - calories = 30\n            }\n        }\n"),
        "{}",
        saved
    );
    assert_eq!(saved_again(&root), saved);
}

#[test]
fn an_entry_written_the_long_way_is_left_as_it_is() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let day = DAY.replace(
        "            - name = \"Soup\"\n",
        "            - name = \"Soup\"\n            <foods.os/Nutrients> per_100g {\n                - calories = 45\n            }\n",
    );
    let root = folder("long", &[("foods.os", FOODS), ("d.os", &day)]);
    assert_eq!(number(&root, "day/Meals/Meal__1/per_100g/calories"), 45.0);
    assert_eq!(saved_again(&root), day);
}
