//! A list inside a named div of a list entry is written when something is added to it.
//!
//! An entry is written as what it overrides of its template, and a named div it does not override
//! was left out whole - with whatever had just been added to a list inside it. So the first comment
//! on a project item changed the document in memory, answered that it had written, and reached
//! the file nowhere; only an entry whose block already said `div Talk { list Comments { ... } }`
//! by hand took a second one. The divs between a change and its entry are written now, holding
//! only what changed.

use overseer::app_api;
use overseer::types::*;

const DOCUMENT: &str = r#"tab project (label="P", mutable=true) {

    div (hidden=true) {
        div Thing {
            string text = ""
        }

        div Row (layout="vertical") {
            string name = ""
            div Box {
                div Deeper {
                    list Things (entry=<Thing>) { }
                }
                div Form {
                    textbox text = ""
                    button add {
                        on click {
                            append (list="../../Deeper/Things", from="..")
                        }
                    }
                }
            }
        }
    }

    list Rows (entry=<Row>, key="name") {
        - {
            - name = "a"
        }
        - {
            - name = "b"
        }
    }
}
"#;

fn a_document(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_entry_div_list_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    file.to_string_lossy().to_string()
}

fn opened(file: &str) -> Vec<OverseerNode> {
    app_api::load_document(std::fs::read_to_string(file).unwrap()).expect("open")
}

fn add(file: &str, row: &str, text: &str) -> bool {
    let nodes = opened(file);
    let at = |address: &str| {
        overseer::addressing::name_path(&nodes, address).unwrap_or_else(|| panic!("nothing at {}", address))
    };
    let typed = vec![app_api::ValueWrite {
        node_path: at(&format!("project/Rows/{}/Box/Form/text", row)),
        value: OverseerValue::String(text.into()),
    }];
    app_api::run_event_at(file, "p.os", at(&format!("project/Rows/{}/Box/Form/add", row)), "click".into(), "s", typed, Vec::new())
        .expect("the press was refused")
        .wrote
}

/// The texts of the things in a row, as the file reads once reopened.
fn things(file: &str, row: &str) -> Vec<String> {
    let nodes = opened(file);
    let list = overseer::addressing::find(&nodes, &format!("project/Rows/{}/Box/Deeper/Things", row)).expect("no list");
    list.children
        .iter()
        .map(|thing| match thing.children.iter().find(|c| c.name == "text").and_then(|c| c.parameters.get("value")) {
            Some(OverseerValue::String(s)) => s.clone(),
            other => format!("{:?}", other),
        })
        .collect()
}

#[test]
fn the_first_thing_added_reaches_the_file() {
    let file = a_document("first");
    assert!(add(&file, "[b]", "one"), "the press says it wrote nothing");
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.contains("div Box {"), "the entry does not say its div:\n{}", written);
    assert_eq!(things(&file, "[b]"), ["one"]);
    assert!(things(&file, "[a]").is_empty(), "the other row was given it too");
}

#[test]
fn the_entry_holds_only_what_changed() {
    let file = a_document("only");
    add(&file, "[a]", "one");
    let written = std::fs::read_to_string(&file).unwrap();
    let entry = &written[written.find("- name = \"a\"").unwrap()..written.find("- name = \"b\"").unwrap()];
    assert!(!entry.contains("div Form"), "the form was written into the entry:\n{}", entry);
    assert!(!entry.contains("textbox"), "the form was written into the entry:\n{}", entry);
}

#[test]
fn later_things_go_after_the_first() {
    let file = a_document("later");
    add(&file, "[a]", "one");
    add(&file, "[a]", "two");
    add(&file, "[a]", "three");
    assert_eq!(things(&file, "[a]"), ["one", "two", "three"]);
}
