//! A list inside an unnamed div of a list entry is written when something is added to it.
//!
//! A div without a name is no step of any address, and an entry writes what is inside one as its
//! own. The writer did that for values and named divs and passed over everything else, so a list
//! kept in an unnamed div for layout took an entry in memory, answered that it had written, and
//! the file did not change. It is written now as the entry's own `list Things { ... }`, which is
//! what its address says and where reading the entry back looks for it.

use overseer::app_api;
use overseer::types::*;

/// A document whose rows keep a list, and a form adding to it, inside `body`.
fn document(body: &str) -> String {
    format!(
        r#"tab project (label="P", mutable=true) {{

    div (hidden=true) {{
        div Thing {{
            string text = ""
        }}

        div Row (layout="vertical") {{
            string name = ""
{}
        }}
    }}

    list Rows (entry=<Row>, key="name") {{
        - {{
            - name = "a"
        }}
        - {{
            - name = "b"
        }}
    }}
}}
"#,
        body
    )
}

const PLAIN: &str = r#"            div {
                list Things (entry=<Thing>) { }
            }
            div Form {
                textbox text = ""
                button add {
                    on click {
                        append (list="../../Things", from="..")
                    }
                }
            }"#;

const IN_A_NAMED_DIV: &str = r#"            div Box {
                div {
                    list Things (entry=<Thing>) { }
                }
                div Form {
                    textbox text = ""
                    button add {
                        on click {
                            append (list="../../Things", from="..")
                        }
                    }
                }
            }"#;

const BESIDE_A_VALUE: &str = r#"            div {
                number count = 0
                list Things (entry=<Thing>) { }
            }
            div Form {
                textbox text = ""
                button add {
                    on click {
                        append (list="../../Things", from="..")
                        set (path="../../count") = 1
                    }
                }
            }"#;

fn a_document(tag: &str, body: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_entry_wrapper_list_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, document(body)).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    file.to_string_lossy().to_string()
}

fn opened(file: &str) -> Vec<OverseerNode> {
    app_api::load_document(std::fs::read_to_string(file).unwrap()).expect("open")
}

/// Type `text` into a row's form and press its button; `form` is the form's address in the row.
fn add(file: &str, row: &str, form: &str, text: &str) -> bool {
    let nodes = opened(file);
    let at = |address: &str| {
        overseer::addressing::name_path(&nodes, address).unwrap_or_else(|| panic!("nothing at {}", address))
    };
    let typed = vec![app_api::ValueWrite {
        node_path: at(&format!("project/Rows/{}/{}/text", row, form)),
        value: OverseerValue::String(text.into()),
    }];
    app_api::run_event_at(file, "p.os", at(&format!("project/Rows/{}/{}/add", row, form)), "click".into(), "s", typed, Vec::new())
        .expect("the press was refused")
        .wrote
}

/// The texts of the things at `list` in a row, as the file reads once reopened.
fn things(file: &str, row: &str, list: &str) -> Vec<String> {
    let nodes = opened(file);
    let list = overseer::addressing::find(&nodes, &format!("project/Rows/{}/{}", row, list)).expect("no list");
    list.children
        .iter()
        .map(|thing| match thing.children.iter().find(|c| c.name == "text").and_then(|c| c.parameters.get("value")) {
            Some(OverseerValue::String(s)) => s.clone(),
            other => format!("{:?}", other),
        })
        .collect()
}

/// What the file says of row `a`.
fn entry_a(file: &str) -> String {
    let written = std::fs::read_to_string(file).unwrap();
    written[written.find("- name = \"a\"").unwrap()..written.find("- name = \"b\"").unwrap()].to_string()
}

#[test]
fn the_first_thing_added_reaches_the_file() {
    let file = a_document("first", PLAIN);
    assert!(add(&file, "[b]", "Form", "one"), "the press says it wrote nothing");
    assert_eq!(things(&file, "[b]", "Things"), ["one"]);
    assert!(things(&file, "[a]", "Things").is_empty(), "the other row was given it too");
}

#[test]
fn the_list_is_written_as_the_entrys_own() {
    let file = a_document("own", PLAIN);
    add(&file, "[a]", "Form", "one");
    let entry = entry_a(&file);
    let expected = [
        "- name = \"a\"",
        "            list Things {",
        "                - {",
        "                    - text = \"one\"",
        "                }",
        "            }",
        "        }",
        "        - {",
        "            ",
    ]
    .join("\n");
    assert_eq!(entry, expected);
}

#[test]
fn later_things_go_after_the_first() {
    let file = a_document("later", PLAIN);
    add(&file, "[a]", "Form", "one");
    add(&file, "[a]", "Form", "two");
    add(&file, "[a]", "Form", "three");
    assert_eq!(things(&file, "[a]", "Things"), ["one", "two", "three"]);
}

#[test]
fn a_list_in_an_unnamed_div_of_a_named_one_is_written() {
    let file = a_document("named", IN_A_NAMED_DIV);
    assert!(add(&file, "[a]", "Box/Form", "one"), "the press says it wrote nothing");
    add(&file, "[a]", "Box/Form", "two");
    assert_eq!(things(&file, "[a]", "Box/Things"), ["one", "two"]);
    let entry = entry_a(&file);
    assert!(entry.contains("div Box {"), "the entry does not say its div:\n{}", entry);
    assert!(!entry.contains("div Form"), "the form was written into the entry:\n{}", entry);
}

#[test]
fn a_list_beside_a_written_value_is_written_with_it() {
    let file = a_document("beside", BESIDE_A_VALUE);
    add(&file, "[a]", "Form", "one");
    assert_eq!(things(&file, "[a]", "Things"), ["one"]);
    let entry = entry_a(&file);
    assert!(entry.contains("- count = 1"), "the value was not written:\n{}", entry);
}

#[test]
fn a_second_save_gives_back_the_first() {
    for (tag, body, form) in [("again_plain", PLAIN, "Form"), ("again_named", IN_A_NAMED_DIV, "Box/Form"), ("again_beside", BESIDE_A_VALUE, "Form")] {
        let file = a_document(tag, body);
        add(&file, "[a]", form, "one");
        let first = std::fs::read_to_string(&file).unwrap();
        let nodes = opened(&file);
        let again = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("save");
        assert_eq!(again, first, "{} changed on its second save", tag);
    }
}
