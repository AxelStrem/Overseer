//! A button inside an unnamed instance of a template declared earlier in the same container is
//! pressed, and appends to the list beside it.
//!
//! The instance is named by its template's name, `Section`, on the way to anything inside it, and
//! the template itself - in a hidden div before it - has that name too. The lookup took the first
//! `Section` it met and gave up when what came next was not in it, so neither the button nor the
//! list could be reached.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;
use std::path::{Path, PathBuf};

const BASE: &str = r##"tab drawer (label="Drawer", mutable=true) {
    div (hidden=true) {
        div Section (layout="vertical") {
            text heading = ""
        }
        div Place { string name = "" }
    }
    <Section> {
        list Places (entry=<Place>, key="name") { }
        button add {
            on click {
                append (list="../Places") { - name = "New" }
            }
        }
    }
}
"##;

fn folder(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_instance_lookup_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("base.os"), BASE).unwrap();
    std::fs::write(root.join("d.os"), "<base.os/drawer> drawer { }\n").unwrap();
    root
}

fn names(root: &Path, document: &str) -> Vec<String> {
    let service = DocumentRoot::new(root).unwrap();
    let list = service.read_at(document, "drawer/Places").expect("the list").node;
    list.children
        .iter()
        .filter_map(|entry| entry.children.iter().find(|c| c.name == "name"))
        .filter_map(|field| match field.parameters.get("value") {
            Some(OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_press_on_a_button_in_the_instance_appends_to_the_list_beside_it() {
    let _guard = serial();
    let root = folder("press");
    DocumentRoot::new(&root).unwrap().run_event("base.os", "drawer/add", "click").expect("the press");

    let saved = std::fs::read_to_string(root.join("base.os")).unwrap();
    assert_eq!(
        saved,
        BASE.replace(
            "list Places (entry=<Place>, key=\"name\") { }",
            "list Places (entry=<Place>, key=\"name\") {\n            - {\n                - name = \"New\"\n            }\n        }"
        ),
        "the entry is written into the instance's list:\n{}",
        saved
    );
    assert_eq!(names(&root, "base.os"), vec!["New".to_string()]);
}

#[test]
fn an_append_at_the_list_in_the_instance_writes_there() {
    let _guard = serial();
    let root = folder("append_at");
    let mut fields = std::collections::HashMap::new();
    fields.insert("name".to_string(), OverseerValue::String("Tasks".to_string()));
    DocumentRoot::new(&root).unwrap().append_at("d.os", "drawer/Places", &fields).expect("the append");

    let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert!(!saved.contains("Section"), "the instance is no step of the file:\n{}", saved);
    assert_eq!(names(&root, "d.os"), vec!["Tasks".to_string()]);
}
