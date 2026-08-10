//! A comment following a list that has no body must keep its place when the file is written.
//!
//! `list intake (entry=<MealRecord>, layout="vertical")` declares a list and stops - no braces,
//! no body. The comment after it belongs to the node that follows. Saving a document whose
//! region was replayed from the source keeps it; saving one that had to be rebuilt - which is
//! what any edit inside the surrounding node forces - dropped both its blank lines and its
//! indentation, so the file drifted every time anything near it changed.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::OverseerNode;

fn load_and_save(text: &str) -> String {
    let nodes = app_api::load_document(text.to_string()).expect("load");
    let across_ipc: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&nodes).expect("to json")).expect("from json");
    let serialized = OverseerFileHandler::serialize_nodes(&across_ipc).expect("serialize");
    app_api::canonicalize_document(&serialized)
}

const DOCUMENT: &str = "\
tab t (label=\"T\") {
    div (hidden=true) {
        div Thing (layout=\"vertical\") {
            float amount (hidden=true) = 0
        }
        div Scorer (layout=\"horizontal\", margin=0) {
            float input (hidden=true) = 0
            float out (label=\"Out\") = $(input * 2)
        }
    }

    div Day (layout=\"vertical\") {
        <Scorer> quality {
            - input = $(1 + 1)
        }

        list intake (entry=<Thing>, layout=\"vertical\")


        // A comment about the button, two blank lines above it.
        button add (label=\"Add\") {
            on click {
                append (list=\"../intake\")
            }
        }
    }
}
";

#[test]
#[ignore = "known: a template instance in a block costs a later sibling's comment its indentation"]
fn a_comment_after_a_childless_list_keeps_its_indentation() {
    let saved = load_and_save(DOCUMENT);
    assert!(
        saved.contains("        // A comment about the button"),
        "the comment lost its indentation:\n{}",
        saved
    );
}

#[test]
fn saving_twice_changes_nothing() {
    let once = load_and_save(DOCUMENT);
    let twice = load_and_save(&once);
    assert_eq!(once, twice, "the document drifts between saves");
}
