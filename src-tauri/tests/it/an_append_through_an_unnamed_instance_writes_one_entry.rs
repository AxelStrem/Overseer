//! A list inside an unnamed instance of a template, in a document made from another, takes an
//! appended entry as one entry.
//!
//! An instance without a name is layout only, as an unnamed div is: no step of any address. The
//! writer of a document made from a base walked through an unnamed div and stopped at an unnamed
//! instance, so it wrote the instance whole as `- Section { - Places { ... } }` and left the
//! document's own `- Places { ... }` beside it - two overrides of one list.

use crate::common::serial;
use overseer::server::DocumentRoot;
use std::path::{Path, PathBuf};

const BASE: &str = r##"tab drawer (label="Drawer", mutable=true) {
    div (hidden=true) {
        div Section (layout="vertical") {
            text heading = ""
        }
        div Place {
            string name = ""
            string path = ""
        }
    }
    <Section> {
        - heading = "DOCUMENTS"
        list Places (entry=<Place>, key="name") { }
    }
    button add (label="Add") {
        on click {
            append (list="../Places") {
                - name = "Other"
            }
        }
    }
    button more (label="More") {
        on click {
            append (list="../Places") {
                - name = "More"
            }
        }
    }
}
"##;

const KEEPING: &str = r##"<base.os/drawer> drawer {
    - Places {
        - {
            - name = "Tasks"
            - path = "tasks.os"
        }
    }
}
"##;

fn folder(test: &str, mine: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_instance_append_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("base.os"), BASE).unwrap();
    std::fs::write(root.join("d.os"), mine).unwrap();
    root
}

fn on_disk(root: &Path) -> String {
    std::fs::read_to_string(root.join("d.os")).unwrap()
}

fn names(root: &Path) -> Vec<String> {
    let service = DocumentRoot::new(root).unwrap();
    let list = service.read_at("d.os", "drawer/Places").expect("the list").node;
    list.children
        .iter()
        .filter_map(|entry| entry.children.iter().find(|c| c.name == "name"))
        .filter_map(|field| match field.parameters.get("value") {
            Some(overseer::types::OverseerValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_append_to_a_list_the_document_overrides_adds_one_entry() {
    let _guard = serial();
    let root = folder("keeping", KEEPING);
    DocumentRoot::new(&root).unwrap().run_event("d.os", "drawer/add", "click").expect("the press");

    assert_eq!(
        on_disk(&root),
        KEEPING.replace(
            "            - path = \"tasks.os\"\n        }\n",
            "            - path = \"tasks.os\"\n        }\n        - {\n            - name = \"Other\"\n        }\n",
        )
    );
    assert_eq!(names(&root), vec!["Tasks".to_string(), "Other".to_string()]);
}

#[test]
fn an_append_to_a_list_the_document_never_named_writes_it_under_the_document() {
    let _guard = serial();
    let root = folder("empty", "<base.os/drawer> drawer { }\n");
    DocumentRoot::new(&root).unwrap().run_event("d.os", "drawer/add", "click").expect("the press");

    let saved = on_disk(&root);
    assert!(!saved.contains("Section"), "the instance is no step of the file:\n{}", saved);
    assert_eq!(saved.matches("Places {").count(), 1, "one override of the list:\n{}", saved);
    assert_eq!(names(&root), vec!["Other".to_string()]);

    DocumentRoot::new(&root).unwrap().run_event("d.os", "drawer/more", "click").expect("a second press");
    let again = on_disk(&root);
    assert!(!again.contains("Section"), "the instance is no step of the file:\n{}", again);
    assert_eq!(again.matches("Places {").count(), 1, "one override of the list:\n{}", again);
    assert_eq!(names(&root), vec!["Other".to_string(), "More".to_string()]);
}
