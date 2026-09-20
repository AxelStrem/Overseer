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

/// Both tests here parse the same text, and two things that outlive a parse are keyed by it: the
/// registry holding each node's source snapshot, and the cache of documents already worked out.
/// Run together they hand each other their nodes - and since `load_and_save` sends the nodes
/// through the IPC boundary, which drops the snapshots and leaves the registry to supply them,
/// whichever test got there first decides the answer. It failed seven runs in ten.
///
/// Serialised and started from nothing, rather than given different documents to read: these are
/// about one particular document, and both of them reading it is the point.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn from_a_clean_slate<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    overseer::source_registry::SourceRegistry::reset();
    app_api::forget_baseline();
    let out = body();
    drop(guard);
    out
}

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

/// The ignore that used to sit here blamed the template instance, which was half the story: it
/// took both the instance and the bodiless list, and neither alone.
///
/// The instance was only what made it visible. Without one the whole block replays from the
/// source verbatim, comment and all, and nothing is noticed. With one the block has to be
/// rebuilt child by child - and the rebuild had nothing to rebuild the comment's place from,
/// because the parser had swallowed it. `list intake (...)` with no body read the whitespace
/// after its header as the gap on the way to looking for a `{`; there being no `{`, that
/// whitespace was the separation from whatever came next, and keeping it left it belonging to
/// nobody - not this node's trailing trivia, and not the next node's leading trivia.
#[test]
fn a_comment_after_a_childless_list_keeps_its_indentation() {
    from_a_clean_slate(|| {
        let saved = load_and_save(DOCUMENT);
        assert!(
            saved.contains("        // A comment about the button"),
            "the comment lost its indentation:\n{}",
            saved
        );
        // The two blank lines above it went the same way and for the same reason.
        assert!(
            saved.contains("\n\n\n        // A comment about the button"),
            "the blank lines above the comment were lost:\n{}",
            saved
        );
    });
}

#[test]
fn saving_twice_changes_nothing() {
    from_a_clean_slate(|| {
        let once = load_and_save(DOCUMENT);
        let twice = load_and_save(&once);
        assert_eq!(once, twice, "the document drifts between saves");
    });
}
