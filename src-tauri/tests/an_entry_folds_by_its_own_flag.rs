//! A foldable div with no name, inside a list's entry, folds by a flag of that entry.
//!
//! A card that opens when it is pressed keeps whether it is open in a field of its own, and the
//! div it folds reads that field by its bare name - `folded=$(open == false)`. The div has no name,
//! so the entry's fields keep their addresses. Each entry's div has to read its own entry's flag:
//! one card opened is one card open.

use overseer::app_api;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Card (layout="vertical") {
            string title (mutable=false) = ""
            bool open (hidden=true) = false
            div (foldable=true, folded=$(open == false)) {
                int n (label="N") = 1
            }
        }
    }
    list Cards (entry=<Card>, key="title") {
        - {
            - title = "a"
        }
        - {
            - title = "b"
            - open = true
        }
        - {
            - title = "c"
        }
    }
}
"#;

fn folded_in(nodes: &[OverseerNode], entry: &str) -> Option<OverseerValue> {
    let at = overseer::delta::indices_of(nodes, &format!("t/Cards/[{}]", entry)).expect("the entry");
    let mut node = &nodes[at[0]];
    for i in &at[1..] {
        node = &node.children[*i];
    }
    let div = node
        .children
        .iter()
        .find(|c| c.parameters.get("foldable") == Some(&OverseerValue::Boolean(true)))
        .expect("the foldable div");
    div.parameters.get("_computed_folded").cloned()
}

#[test]
fn each_entry_reads_its_own_flag() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert_eq!(folded_in(&nodes, "a"), Some(OverseerValue::Boolean(true)));
    assert_eq!(folded_in(&nodes, "b"), Some(OverseerValue::Boolean(false)));
    assert_eq!(folded_in(&nodes, "c"), Some(OverseerValue::Boolean(true)));
}
