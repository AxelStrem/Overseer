//! A form fills an entry whose template is itself made from another, in a file the document is
//! made from.
//!
//! Such a file is read without being worked out, so `<Label> Tag { ... }` was found as only what
//! it writes: a copy into it found none of the fields `Label` gives it, and a list kept by one of
//! them refused the entry for having no key. The project documents' tag form failed this way once
//! their Labels list took entries made from a template of its own.

use crate::common::serial;
use overseer::server::DocumentRoot;
use serde_json::json;

const BASE: &str = r#"tab project (label="P", mutable=true) {
    div (hidden=true) {
        div Label {
            div (layout="horizontal") {
                string tag = ""
                string name = ""
            }
        }
        <Label> Tag {
            int uses = $(../tag == "" ? 0 : 1)
        }
    }

    list Labels (entry=<Tag>, key="tag") { }

    div NewTag (layout="horizontal") {
        textbox tag = ""
        textbox name = ""
        button add (label="+ tag") {
            on click {
                if (cond=$(../tag == "")) {
                    refuse = "say a tag"
                }
                append (list="/project/Labels", from="..")
            }
        }
    }
}
"#;

const DOCUMENT: &str = r#"<base.os/project> project {
    - Labels {
        - {
            - tag = "bug"
        }
    }
}
"#;

#[test]
fn a_form_fills_an_entry_of_a_template_made_from_another() {
    let _serial = serial();
    let root = std::env::temp_dir().join(format!("overseer_stacked_form_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("base.os"), BASE).expect("write the base");
    std::fs::write(root.join("p.os"), DOCUMENT).expect("write the document");
    overseer::viewstate::forget_all();
    let service = DocumentRoot::new(&root).expect("open the root");

    let form = |field: &str| vec!["project".to_string(), "NewTag".into(), field.into()];
    let typed = vec![
        json!({ "node_path": form("tag"), "value": { "String": "perf" } }),
        json!({ "node_path": form("name"), "value": { "String": "speed" } }),
    ];
    service
        .command_for(
            "alice",
            Some("p.os"),
            "run_overseer_event",
            &json!({ "node_path": form("add"), "event_name": "click", "typed": typed }),
        )
        .expect("the press was refused");

    let text = std::fs::read_to_string(root.join("p.os")).expect("read");
    assert!(text.contains("- tag = \"perf\""), "the tag was not copied:\n{text}");
    assert!(text.contains("- name = \"speed\""), "the name was not copied:\n{text}");
    // What the templates give is not written with the entry.
    assert!(!text.contains("uses"), "a field the template works out was written:\n{text}");
    let _ = std::fs::remove_dir_all(&root);
}
