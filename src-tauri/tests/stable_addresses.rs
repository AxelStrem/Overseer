//! Node addresses must survive a resolve, or a change cannot be described without sending the
//! whole document.
//!
//! The ids minted during a parse are positional and change wholesale whenever the text does.
//! These addresses come from the document's structure instead, so the same logical node keeps
//! the same address - including across a reordering of a keyed list, which is the case that
//! makes positional identity useless: exercise.os sorts its exercises by when they were last
//! done, so pressing Done moves one.

use overseer::addressing;
use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

fn open() -> (String, Vec<OverseerNode>) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/exercise_tracker/exercise.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let text = std::fs::read_to_string(&p).unwrap();
    let nodes = app_api::load_document(text.clone()).unwrap();
    (text, nodes)
}

fn value_at(nodes: &[OverseerNode], address: &str, field: &str) -> Option<OverseerValue> {
    addressing::find(nodes, address)?
        .children
        .iter()
        .find(|c| c.name == field)?
        .parameters
        .get("value")
        .cloned()
}

#[test]
fn addresses_are_unique() {
    let (_, nodes) = open();
    let all = addressing::addresses(&nodes);
    let unique: std::collections::HashSet<_> = all.iter().collect();
    assert_eq!(
        unique.len(),
        all.len(),
        "{} of {} addresses name more than one node",
        all.len() - unique.len(),
        all.len()
    );
    DocumentManager::set_current_document(None);
}

#[test]
fn an_edit_leaves_every_address_alone() {
    let (text, before) = open();
    let field = "exercise_tracker/Exercises/Exercise__1/plates/p4";
    let mut m = std::collections::HashMap::new();
    m.insert(field.to_string(), OverseerValue::Integer(3));
    let after = app_api::resolve_selective(text, vec![field.to_string()], Some(m)).unwrap();

    assert_eq!(
        addressing::addresses(&before),
        addressing::addresses(&after),
        "editing a value changed the addresses of the document"
    );
    DocumentManager::set_current_document(None);
}

const KEYED: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Row (layout="vertical") {
            string id = ""
            int qty = 0
        }
    }
    list Rows (entry=<Row>, key="id") {
        - {
            - id = "a"
            - qty = 1
        }
        - {
            - id = "b"
            - qty = 2
        }
    }
}
"#;

#[test]
fn a_keyed_entry_keeps_its_address_when_one_is_inserted_before_it() {
    DocumentManager::set_current_document(None);
    let before = app_api::load_document(KEYED.to_string()).unwrap();
    let addresses_before = addressing::addresses(&before);

    // A new entry at the front, as prepending a record to a history does.
    let inserted = KEYED.replace(
        "    list Rows (entry=<Row>, key=\"id\") {
",
        "    list Rows (entry=<Row>, key=\"id\") {
        - {
            - id = \"z\"
            - qty = 9
        }
",
    );
    assert_ne!(inserted, KEYED, "the fixture did not change");
    let after = app_api::load_document(inserted).unwrap();
    let addresses_after = addressing::addresses(&after);

    for existing in ["t/Rows/[a]", "t/Rows/[b]", "t/Rows/[b]/qty"] {
        assert!(
            addresses_before.contains(&existing.to_string()),
            "{} was not addressable before the insertion, so this proves nothing",
            existing
        );
        assert!(
            addresses_after.contains(&existing.to_string()),
            "{} lost its address when an entry was inserted before it",
            existing
        );
    }

    // The entry did move, so a positional address would have named its neighbour.
    let position = |all: &[String], a: &str| all.iter().position(|x| x == a).unwrap();
    assert_ne!(
        position(&addresses_before, "t/Rows/[b]"),
        position(&addresses_after, "t/Rows/[b]"),
        "nothing shifted, so the keyed address was never tested"
    );
    assert_eq!(
        value_at(&after, "t/Rows/[b]", "qty"),
        Some(OverseerValue::Integer(2)),
        "the address names a different entry than it did before the insertion"
    );
}

#[test]
fn an_unnamed_wrapper_is_not_part_of_an_address() {
    // A `div` written without a name groups things for layout, and the rest of the system
    // looks straight through it - formula paths, rendered paths, the frontend's own lookups.
    // An address that mentioned it would be the odd one out, saying `div#2` where everything
    // else says nothing at all.
    let (_, nodes) = open();
    let all = addressing::addresses(&nodes);

    assert!(
        all.iter().any(|a| a == "exercise_tracker/Exercises"),
        "the list is not addressable without naming the wrapper around it"
    );
    assert!(
        !all.iter().any(|a| a.contains("/div")),
        "an unnamed wrapper appears in an address: {:?}",
        all.iter().find(|a| a.contains("/div"))
    );
    DocumentManager::set_current_document(None);
}

#[test]
fn a_named_container_keeps_its_place_in_an_address() {
    // The question is not whether a node affects rendering - a `tab` is transparent too - but
    // whether anyone would call it something. `tracker_v2` is a name; `div` is not.
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/weight_tracker/tracker_v2.os");
    DocumentManager::set_current_document(Some(p.to_string_lossy().as_ref()));
    let nodes = app_api::load_document(std::fs::read_to_string(&p).unwrap()).unwrap();
    let all = addressing::addresses(&nodes);
    assert!(
        all.iter().any(|a| a.starts_with("tracker_v2/History")),
        "the named tab was dropped from its own addresses"
    );
    DocumentManager::set_current_document(None);
}
