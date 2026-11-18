//! A list's entry template can hold an instance of another template and restyle a field in the
//! instance's block, `<Nutrients> per_100g { - protein (font-size=12px) }`. The style is the
//! entry template's: an edit of the field on one entry writes the value it was given, not the
//! style beside it.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const DAY: &str = r##"tab day (label="Day", mutable=true) {
    div (hidden=true) {
        div Nutrients {
            float protein (label="Protein", suffix=" g") = 5
            float fibre (label="Fibre") = 0
        }
        div Meal {
            string name = ""
            <Nutrients> per_100g {
                - protein (font-size=12px, layout="vertical", margin=0, padding=0)
                - fibre = 1
            }
        }
    }
    list meals (entry=<Meal>) {
        - {
            - name = "cake"
        }
    }
}
"##;

const FOODS: &str = r##"tab food_catalog (label="Foods", mutable=true) {
    div (hidden=true) {
        div Nutrients {
            float protein (label="Protein", suffix=" g") = 5
            float fibre (label="Fibre") = 0
        }
        div Food {
            string handle = "unnamed"
            <Nutrients> per_100g { }
        }
    }

    list Catalog (entry=<Food>, key="handle") {
        - {
            - handle = "cake"
            - per_100g {
                - protein = 6
            }
        }
    }
}
"##;

const STACKED: &str = r##"tab day (label="Day", mutable=true) {
    div (hidden=true) {
        div Meal {
            string name = ""
            <foods.os/Nutrients> per_100g {
                - protein (font-size=12px, layout="vertical", margin=0, padding=0)
            }
        }
    }

    mount FOODS (hidden=true, lazy=false, mutable=false, source="foods.os/food_catalog/Catalog") { }

    list meals (entry=<Meal>) {
        - <FOODS/Catalog/[cake]> {
            - name = "cake"
        }
    }
}
"##;

fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_template_style_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

fn on_disk(root: &Path) -> String {
    std::fs::read_to_string(root.join("d.os")).unwrap()
}

fn saved_again(root: &Path) -> String {
    let content = on_disk(root);
    let nodes = overseer::docmgr::manager::DocumentManager::with_document(Some(root.to_path_buf()), || {
        let (_rest, mut nodes) = overseer::parser::parse_document(&content).unwrap();
        overseer::resolver::resolve_document(&mut nodes);
        nodes
    });
    overseer::OverseerFileHandler::serialize_nodes(&nodes).unwrap()
}

fn protein(root: &Path) -> OverseerNode {
    DocumentRoot::new(root)
        .unwrap()
        .read_at("d.os", "day/meals/Meal__1/per_100g/protein")
        .expect("protein")
        .node
}

fn number(node: &OverseerNode) -> f64 {
    match node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")) {
        Some(OverseerValue::Float(f)) => *f,
        Some(OverseerValue::Integer(n)) => *n as f64,
        other => panic!("protein reads {:?}", other),
    }
}

#[test]
fn an_edit_of_a_restyled_field_writes_the_value_alone() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("plain", &[("d.os", DAY)]);
    let service = DocumentRoot::new(&root).unwrap();
    service
        .set_at("d.os", "day/meals/Meal__1/per_100g/protein", OverseerValue::Float(4.0))
        .unwrap();

    let saved = on_disk(&root);
    assert_eq!(
        saved,
        DAY.replace(
            "            - name = \"cake\"\n",
            "            - name = \"cake\"\n            - per_100g {\n                - protein = 4\n            }\n"
        ),
        "{}",
        saved
    );
    let field = protein(&root);
    assert_eq!(number(&field), 4.0);
    assert!(field.parameters.contains_key("font-size"), "{:?}", field.parameters);
    assert_eq!(saved_again(&root), saved);
}

#[test]
fn the_entry_template_keeps_its_block() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("untouched", &[("d.os", DAY)]);
    assert_eq!(saved_again(&root), DAY);
    let field = protein(&root);
    assert_eq!(number(&field), 5.0);
    assert!(field.parameters.contains_key("font-size"), "{:?}", field.parameters);
}

#[test]
fn an_entry_made_from_a_catalogue_entry_writes_the_value_alone() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("stacked", &[("foods.os", FOODS), ("d.os", STACKED)]);
    assert_eq!(number(&protein(&root)), 6.0);
    let service = DocumentRoot::new(&root).unwrap();
    service
        .set_at("d.os", "day/meals/Meal__1/per_100g/protein", OverseerValue::Float(4.0))
        .unwrap();

    let saved = on_disk(&root);
    assert_eq!(
        saved,
        STACKED.replace(
            "            - name = \"cake\"\n",
            "            - name = \"cake\"\n            - per_100g {\n                - protein = 4\n            }\n"
        ),
        "{}",
        saved
    );
    let field = protein(&root);
    assert_eq!(number(&field), 4.0);
    assert!(field.parameters.contains_key("font-size"), "{:?}", field.parameters);
    assert_eq!(saved_again(&root), saved);
}

#[test]
fn a_style_the_entry_writes_itself_stays_the_entrys() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let day = DAY.replace(
        "            - name = \"cake\"\n",
        "            - name = \"cake\"\n            - per_100g {\n                - protein (font-size=14px) = 4\n            }\n",
    );
    let root = folder("own", &[("d.os", &day)]);
    assert_eq!(saved_again(&root), day);
    let service = DocumentRoot::new(&root).unwrap();
    service
        .set_at("d.os", "day/meals/Meal__1/per_100g/protein", OverseerValue::Float(3.0))
        .unwrap();
    let saved = on_disk(&root);
    assert_eq!(saved, day.replace("(font-size=14px) = 4", "(font-size=14px) = 3"), "{}", saved);
    assert_eq!(saved_again(&root), saved);
}
