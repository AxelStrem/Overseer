//! A document's templates are instantiated in one pass.
//!
//! A pass walks into what it has just instantiated, so a list or an instance inside a template's
//! body is resolved along with the entry that brought it in. A second pass used to run every time
//! anyway, only to find nothing had moved - a clone of the instantiated tree and a rebuild of every
//! entry, a fifth of what opening the task list cost.

use overseer::parser::parse_document;
use overseer::resolver::{resolve_document, template_passes_taken};
use overseer::types::{OverseerNode, OverseerValue};

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(f) = find(&n.children, name) {
            return Some(f);
        }
    }
    None
}

fn resolved(text: &str) -> Vec<OverseerNode> {
    let mut nodes = parse_document(text).expect("parses").1;
    resolve_document(&mut nodes);
    nodes
}

const NESTED: &str = r#"tab t (label="T") {
    div (hidden=true) {
        div Step {
            string what = "a step"
        }
        div Owner {
            string who = "nobody"
        }
        div Task {
            string title = ""
            <Owner> owner { }
            list Steps (entry=<Step>) { }
        }
    }
    list Tasks (entry=<Task>) {
        - {
            - title = "first"
            - Steps {
                - {
                    - what = "write it"
                }
                - {
                    - what = "test it"
                }
            }
        }
        - {
            - title = "second"
        }
    }
}
"#;

#[test]
fn templates_inside_a_template_are_instantiated_in_the_same_pass() {
    let nodes = resolved(NESTED);
    assert_eq!(template_passes_taken(), 1);

    let tasks = find(&nodes, "Tasks").expect("no Tasks list");
    assert_eq!(tasks.children.len(), 2);
    for task in &tasks.children {
        assert_eq!(task.node_type, "Task");
        let owner = find(&task.children, "owner").expect("no owner");
        assert_eq!(owner.node_type, "Owner");
        assert!(find(&owner.children, "who").is_some(), "the owner was not instantiated");
    }
    let steps = find(&tasks.children[0].children, "Steps").expect("no Steps list");
    assert_eq!(steps.children.len(), 2);
    for step in &steps.children {
        assert_eq!(step.node_type, "Step");
    }
    let what = find(&steps.children[1].children, "what").expect("no what");
    assert_eq!(
        what.parameters.get("value"),
        Some(&OverseerValue::String("test it".to_string()))
    );
}

#[test]
fn a_template_that_is_nowhere_does_not_cost_a_second_pass() {
    let nodes = resolved(
        r#"tab t (label="T") {
    div (hidden=true) {
        div Day {
            int day = 0
        }
    }
    list Days (entry=<Day>) {
        - {
            - day = 1
        }
    }
    list Elsewhere (entry=<Missing>) {
        - {
            - day = 2
        }
    }
}
"#,
    );
    assert_eq!(template_passes_taken(), 1);
    let days = find(&nodes, "Days").expect("no Days list");
    assert_eq!(days.children[0].node_type, "Day");
}

#[test]
fn a_list_inside_a_wrapper_gets_its_sort_keys() {
    let nodes = resolved(
        r#"tab t (label="T") {
    div (hidden=true) {
        div Day {
            int day = 0
        }
    }
    div {
        list Days (entry=<Day>, sort_by=$(|x| 0 - x/day)) {
            - {
                - day = 1
            }
            - {
                - day = 3
            }
            - {
                - day = 2
            }
        }
    }
}
"#,
    );
    let days = find(&nodes, "Days").expect("no Days list");
    let keys: Vec<Option<&OverseerValue>> = days
        .children
        .iter()
        .map(|d| d.parameters.get("_ui_sort_key"))
        .collect();
    assert_eq!(
        keys,
        vec![
            Some(&OverseerValue::Integer(-1)),
            Some(&OverseerValue::Integer(-3)),
            Some(&OverseerValue::Integer(-2)),
        ]
    );
}
