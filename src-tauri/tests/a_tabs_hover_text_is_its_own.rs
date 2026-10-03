//! What a tab says on hover is about the tab, and stays on its header.
//!
//! `hover-text` is handed down from a container to everything inside it, so a table can ask once
//! and every cell answer with the name of its column. A tab's is what its header says about the
//! tab - "what is still to do" - and handed down, every field on the page would say the same about
//! itself. So a tab keeps its own; what it was handed from above goes on down as through anything
//! else.

use overseer::app_api;
use overseer::types::{OverseerNode, OverseerValue};

const DOCUMENT: &str = r#"tab t (label="T") {
    div plan (hover-text=true) {
        tab open (label="Open", hover-text="What is still to do") {
            string first = "x"
        }
        tab done (label="Done") {
            string third = "y"
        }
    }

    div (hidden=true) {
        tab Day (label=$(date), hover-text="A day") {
            string date = ""
        }
    }

    list Days (entry=<Day>, key="date") {
        - {
            - date = "2026-10-01"
        }
    }
}
"#;

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes.iter().find_map(|n| if n.name == name { Some(n) } else { find(&n.children, name) })
}

fn says(nodes: &[OverseerNode], name: &str) -> Option<OverseerValue> {
    find(nodes, name).unwrap_or_else(|| panic!("no {}", name)).parameters.get("hover-text").cloned()
}

#[test]
fn a_tab_keeps_what_it_says_for_its_header() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert_eq!(says(&nodes, "open"), Some(OverseerValue::String("What is still to do".into())));
}

#[test]
fn the_fields_on_its_page_do_not_say_it() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert_ne!(
        says(&nodes, "first"),
        Some(OverseerValue::String("What is still to do".into())),
        "a field on the page took the tab's hover text for its own"
    );
}

#[test]
fn what_the_tab_was_handed_still_goes_on_down() {
    // `plan` asks every field inside it to say its own name on hover, tabs or no tabs.
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert_eq!(says(&nodes, "first"), Some(OverseerValue::Boolean(true)));
    assert_eq!(says(&nodes, "third"), Some(OverseerValue::Boolean(true)));
}

#[test]
fn a_tab_made_from_a_template_keeps_its_own_too() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let days = find(&nodes, "Days").expect("no Days");
    let entry = &days.children[0];
    let date = find(&entry.children, "date").expect("no date");
    assert_ne!(
        date.parameters.get("hover-text"),
        Some(&OverseerValue::String("A day".into())),
        "the entry's field took the tab's hover text"
    );
}
