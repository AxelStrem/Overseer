//! A list entry can have more stacked on its list's template: `- <Catalog/[apple]> { ... }` is
//! the list's template, then what the catalogue's `apple` entry writes, then the entry's own block.
//!
//! The entry named gives only its own lines - its values, and none of the labels and widths its
//! own list's template has - so a record made from a catalogue entry looks like a record. It is
//! named in the document, in another file, or through a mount of that file's list. Nothing the
//! stack gave is written back with the entry: a save gives back what was written, and an edit of
//! an inherited value becomes one override.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};


const FOODS: &str = r##"tab food_catalog (label="Foods", mutable=true) {
    div (hidden=true) {
        div Food (layout="horizontal") {
            string handle (label="Handle", width=15%) = "unnamed"
            string name (label="Name", width=25%) = ""
            float portion (label="Portion") = 100
            div per_100g (label="Per 100 g") {
                float calories (label="kcal") = 100
                float protein (label="Protein") = 5
            }
        }
    }

    list Catalog (entry=<Food>, key="handle") {
        - {
            - handle = "apple"
            - name = "Apple"
            - portion = 180
            div per_100g {
                - calories = 52
            }
        }
        - {
            - handle = "bread"
            - name = "Bread"
            - portion = 40
            div per_100g {
                - calories = 250
                - protein = 9
            }
        }
    }
}
"##;

const DAY: &str = r##"tab day (label="Day", mutable=true) {
    div (hidden=true) {
        div Meal {
            string name (label="") = ""
            float portion (hidden=true) = 100
            float portions = 1
            div per_100g (hidden=true) {
                float calories = 0
                float protein = 0
            }
            float kcal = $(portions * portion * per_100g/calories * 0.01)
        }
    }

    mount FOODS (hidden=true, lazy=false, mutable=false, source="foods.os/food_catalog/Catalog") { }

    list Meals (entry=<Meal>) {
        - <FOODS/Catalog/[apple]> {
            - portions = 2
        }
        - <foods.os/food_catalog/Catalog/[bread]> { }
        - {
            - name = "Soup"
        }
        - <FOODS/Catalog/[bread]> {
            - name = "Toast"
            div per_100g {
                - calories = 300
            }
        }
    }
}
"##;

fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_stacked_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

fn read(root: &Path, address: &str) -> OverseerNode {
    let service = DocumentRoot::new(root).unwrap();
    service
        .read_at("d.os", address)
        .unwrap_or_else(|e| panic!("nothing read at {}: {:?}", address, e))
        .node
}

fn value(root: &Path, address: &str) -> OverseerValue {
    let node = read(root, address);
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("no value at {}", address))
}

fn number(root: &Path, address: &str) -> f64 {
    match value(root, address) {
        OverseerValue::Float(f) => f,
        OverseerValue::Integer(n) => n as f64,
        other => panic!("{} reads {:?}", address, other),
    }
}

fn text(root: &Path, address: &str) -> String {
    match value(root, address) {
        OverseerValue::String(s) => s,
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

#[test]
fn the_entry_has_the_catalogue_values_under_its_own() {
    let _guard = serial();
    let root = folder("values", &[("foods.os", FOODS), ("d.os", DAY)]);

    // Through the mount: the catalogue's values, the entry's own amount, the template's formula.
    assert_eq!(text(&root, "day/Meals/Meal__1/name"), "Apple");
    assert_eq!(number(&root, "day/Meals/Meal__1/portion"), 180.0);
    assert_eq!(number(&root, "day/Meals/Meal__1/per_100g/calories"), 52.0);
    assert_eq!(number(&root, "day/Meals/Meal__1/kcal"), 2.0 * 180.0 * 52.0 * 0.01);
    // What the catalogue entry does not write is the record's template's, not the catalogue's.
    assert_eq!(number(&root, "day/Meals/Meal__1/per_100g/protein"), 0.0);

    // By file.
    assert_eq!(text(&root, "day/Meals/Meal__2/name"), "Bread");
    assert_eq!(number(&root, "day/Meals/Meal__2/per_100g/protein"), 9.0);

    // No catalogue entry at all.
    assert_eq!(text(&root, "day/Meals/Meal__3/name"), "Soup");
    assert_eq!(number(&root, "day/Meals/Meal__3/per_100g/calories"), 0.0);

    // The entry's own block wins over the catalogue's.
    assert_eq!(text(&root, "day/Meals/Meal__4/name"), "Toast");
    assert_eq!(number(&root, "day/Meals/Meal__4/per_100g/calories"), 300.0);
    assert_eq!(number(&root, "day/Meals/Meal__4/per_100g/protein"), 9.0);
}

#[test]
fn the_entry_keeps_its_own_templates_look() {
    let _guard = serial();
    let root = folder("look", &[("foods.os", FOODS), ("d.os", DAY)]);
    let name = read(&root, "day/Meals/Meal__1/name");
    assert_eq!(name.parameters.get("label"), Some(&OverseerValue::String(String::new())));
    assert_eq!(name.parameters.get("width"), None, "the catalogue's width came along");
    let figures = read(&root, "day/Meals/Meal__1/per_100g");
    assert_eq!(figures.parameters.get("hidden"), Some(&OverseerValue::Boolean(true)));
    assert_eq!(figures.parameters.get("label"), None);
    // A field of the catalogue's the record has none for comes along, typed as the catalogue has it.
    let handle = read(&root, "day/Meals/Meal__1/handle");
    assert_eq!(handle.node_type, "string");
    assert_eq!(handle.parameters.get("value"), Some(&OverseerValue::String("apple".into())));
}

#[test]
fn a_save_gives_back_what_was_written() {
    let _guard = serial();
    for newline in ["\n", "\r\n"] {
        let day = DAY.replace('\n', newline);
        let root = folder("save", &[("foods.os", FOODS), ("d.os", &day)]);
        let nodes = overseer::docmgr::manager::DocumentManager::with_document(Some(root.clone()), || {
            let (_rest, mut nodes) = overseer::parser::parse_document(&day).unwrap();
            overseer::resolver::resolve_document(&mut nodes);
            nodes
        });
        let saved = overseer::OverseerFileHandler::serialize_nodes(&nodes).unwrap();
        assert_eq!(saved, day);
    }
}

#[test]
fn an_edit_of_an_inherited_value_is_one_override() {
    let _guard = serial();
    let root = folder("edit", &[("foods.os", FOODS), ("d.os", DAY)]);
    let service = DocumentRoot::new(&root).unwrap();

    service
        .set_at("d.os", "day/Meals/Meal__1/name", OverseerValue::String("Green apple".into()))
        .unwrap();
    assert_eq!(
        on_disk(&root),
        DAY.replace(
            "            - portions = 2\n",
            "            - name = \"Green apple\"\n            - portions = 2\n"
        )
    );
    // Written once more untouched, it stays as it is.
    assert_eq!(saved_again(&root), on_disk(&root));
    assert_eq!(text(&root, "day/Meals/Meal__1/name"), "Green apple");
    assert_eq!(number(&root, "day/Meals/Meal__1/portion"), 180.0);

    // An entry that wrote nothing of its own gains its first line.
    service
        .set_at("d.os", "day/Meals/Meal__2/portions", OverseerValue::Float(3.0))
        .unwrap();
    let saved = on_disk(&root);
    assert!(
        saved.contains("        - <foods.os/food_catalog/Catalog/[bread]> {\n            - portions = 3\n        }\n"),
        "{}",
        saved
    );
    assert_eq!(text(&root, "day/Meals/Meal__2/name"), "Bread");
    assert_eq!(number(&root, "day/Meals/Meal__2/kcal"), 3.0 * 40.0 * 250.0 * 0.01);
}

#[test]
fn an_entry_that_is_not_there_is_said_and_the_rest_is_drawn() {
    let _guard = serial();
    let day = DAY.replace("[apple]", "[pear]");
    let root = folder("missing", &[("foods.os", FOODS), ("d.os", &day)]);
    let entry = read(&root, "day/Meals/Meal__1");
    match entry.parameters.get("_inherit_error") {
        Some(OverseerValue::String(why)) => assert!(why.contains("pear"), "{}", why),
        other => panic!("says {:?}", other),
    }
    assert_eq!(number(&root, "day/Meals/Meal__1/portions"), 2.0);
    assert_eq!(text(&root, "day/Meals/Meal__1/name"), "");
    assert_eq!(text(&root, "day/Meals/Meal__2/name"), "Bread");
}

#[test]
fn the_document_follows_the_catalogue() {
    let _guard = serial();
    let root = folder("follows", &[("foods.os", FOODS), ("d.os", DAY)]);
    assert_eq!(number(&root, "day/Meals/Meal__1/per_100g/calories"), 52.0);
    // Long enough apart that the file's stamp moves.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(root.join("foods.os"), FOODS.replace("- calories = 52", "- calories = 60")).unwrap();
    assert_eq!(number(&root, "day/Meals/Meal__1/per_100g/calories"), 60.0);
}

const BUTTONS: &str = r##"
    string picked = "bread"

    button add {
        on click {
            append (list="../Meals", with=$("FOODS/Catalog/[" + ../picked + "]")) {
                - portions = 3
            }
        }
    }

    button first {
        on click {
            prepend (list="../Meals", with=<FOODS/Catalog/[apple]>)
        }
    }

    button none {
        on click {
            append (list="../Meals", with=<FOODS/Catalog/[pear]>)
        }
    }
}
"##;

/// The day with its buttons: the document's last brace gives way to them.
fn day_with_buttons() -> String {
    format!("{}{}", DAY.strip_suffix("}\n").unwrap(), BUTTONS)
}

#[test]
fn an_append_makes_an_entry_from_a_catalogue_entry() {
    let _guard = serial();
    let day = day_with_buttons();
    let root = folder("append", &[("foods.os", FOODS), ("d.os", &day)]);
    let service = DocumentRoot::new(&root).unwrap();

    // What to stack is given by a formula, as a form's choice would give it.
    service.run_event("d.os", "day/add", "click").expect("the press");
    let saved = on_disk(&root);
    assert!(
        saved.contains("            }\n        }\n        - <FOODS/Catalog/[bread]> {\n            - portions = 3\n        }\n    }\n"),
        "{}",
        saved
    );
    assert_eq!(text(&root, "day/Meals/Meal__5/name"), "Bread");
    assert_eq!(number(&root, "day/Meals/Meal__5/kcal"), 3.0 * 40.0 * 250.0 * 0.01);

    // With nothing of its own, at the front.
    service.run_event("d.os", "day/first", "click").expect("the press");
    let saved = on_disk(&root);
    assert!(
        saved.contains("    list Meals (entry=<Meal>) {\n        - <FOODS/Catalog/[apple]> { }\n        - <FOODS/Catalog/[apple]> {\n"),
        "{}",
        saved
    );
    assert_eq!(text(&root, "day/Meals/Meal__1/name"), "Apple");
    assert_eq!(saved_again(&root), saved);

    // One that is not in the catalogue is refused, and nothing is written.
    assert!(service.run_event("d.os", "day/none", "click").is_err());
    assert_eq!(on_disk(&root), saved);
}

#[test]
fn the_server_appends_an_entry_made_from_a_catalogue_entry() {
    let _guard = serial();
    let root = folder("api", &[("foods.os", FOODS), ("d.os", DAY)]);
    let service = DocumentRoot::new(&root).unwrap();
    let mut fields = std::collections::HashMap::new();
    fields.insert("portions".to_string(), OverseerValue::Float(2.0));
    // A field only the catalogue entry brings is one the entry may set.
    fields.insert("handle".to_string(), OverseerValue::String("apple_2".into()));
    service
        .append_at_with("d.os", "day/Meals", &fields, Some("FOODS/Catalog/[apple]"))
        .expect("the append");
    let saved = on_disk(&root);
    assert!(saved.contains("        - <FOODS/Catalog/[apple]> {\n            - portions = 2\n"), "{}", saved);
    assert_eq!(text(&root, "day/Meals/Meal__5/name"), "Apple");
    assert_eq!(text(&root, "day/Meals/Meal__5/handle"), "apple_2");
    assert_eq!(number(&root, "day/Meals/Meal__5/kcal"), 2.0 * 180.0 * 52.0 * 0.01);

    let refused = service.append_at_with("d.os", "day/Meals", &fields, Some("FOODS/Catalog/[pear]"));
    assert!(refused.is_err(), "an entry was made from a food that is not there");
    assert_eq!(on_disk(&root), saved);

    let mut unknown = std::collections::HashMap::new();
    unknown.insert("colour".to_string(), OverseerValue::String("red".into()));
    assert!(service
        .append_at_with("d.os", "day/Meals", &unknown, Some("FOODS/Catalog/[apple]"))
        .is_err());
}

const BASE: &str = r##"div (hidden=true) {
    div Row {
        string name = ""
        float portions = 1
        div per_100g {
            float calories = 0
        }
    }
    div Loud (font-weight="bold") {
        string name = "loud"
    }
    div log {
        <Row, Loud> sample { }
        list Rows (entry=<Row>) {
            - <Loud> { }
            - <foods.os/food_catalog/Catalog/[apple]> { }
        }
    }
}
"##;

const FROM_A_BASE: &str = r##"tab t (label="T", mutable=true) {
    <lib/base.os/log> log { }
}
"##;

#[test]
fn a_stack_written_in_a_base_means_there_what_it_means_in_the_base() {
    let _guard = serial();
    // The base and the catalogue it names sit together, away from the document made from them.
    let root = folder(
        "base",
        &[("lib/foods.os", FOODS), ("lib/base.os", BASE), ("d.os", FROM_A_BASE)],
    );
    assert_eq!(text(&root, "t/log/sample/name"), "loud");
    assert_eq!(text(&root, "t/log/Rows/Row__1/name"), "loud");
    assert_eq!(text(&root, "t/log/Rows/Row__2/name"), "Apple");
    assert_eq!(number(&root, "t/log/Rows/Row__2/per_100g/calories"), 52.0);
    assert_eq!(saved_again(&root), FROM_A_BASE);
}
