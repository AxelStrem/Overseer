//! `mutable`, answered in Rust, so both doors agree about which fields are the viewer's.
//!
//! The parameter has three states and only two of them are about permission. `guarded` says a
//! field is edited by whoever is looking and that the edit is not a fact about anybody: which day
//! the food tracker is showing, whether a card is folded. Until now the page was the only thing
//! that knew, so it worked through `/api`, where the page collects the guarded fields and hands
//! back their authored values at save time, and did nothing at all through `/v1`, where the write
//! is the action and there is no save to intervene in.
//!
//! These pin the reading rather than the acting: inheritance, the three states, the shadow a
//! formula leaves, and what happens when a document says nothing. The acting on it comes next.

use overseer::mutability::{at, is_view_state, Mutability};
use overseer::{app_api, types::OverseerNode};

fn resolved(text: &str) -> Vec<OverseerNode> {
    app_api::load_document(text.to_string()).expect("load")
}

const DOCUMENT: &str = "\
tab tracker (label=\"Tracker\", mutable=true) {
    int amount = 1

    timestamp showing (mutable=\"guarded\") = \"2026-09-19T00:00:00+04:00\"

    div locked (mutable=false) {
        string reading = \"fixed\"
    }

    div plain (layout=\"vertical\") {
        string inherits = \"from the tab\"
    }

    div (layout=\"vertical\", mutable=\"guarded\") {
        string through_a_wrapper = \"\"
    }
}

tab quiet (label=\"Quiet\") {
    string untouched = \"\"
}
";

#[test]
fn a_field_takes_what_its_tab_says() {
    let nodes = resolved(DOCUMENT);
    assert_eq!(at(&nodes, "tracker/amount"), Mutability::Editable);
}

#[test]
fn a_field_can_say_it_is_the_viewers_rather_than_the_documents() {
    let nodes = resolved(DOCUMENT);
    assert_eq!(at(&nodes, "tracker/showing"), Mutability::Guarded);
    assert!(is_view_state(&nodes, "tracker/showing"));
}

#[test]
fn the_nearest_thing_that_says_anything_decides() {
    // The tab says editable; the div inside it says no. The div is nearer.
    let nodes = resolved(DOCUMENT);
    assert_eq!(at(&nodes, "tracker/locked"), Mutability::Fixed);
    assert_eq!(at(&nodes, "tracker/locked/reading"), Mutability::Fixed);
}

#[test]
fn a_div_that_says_nothing_passes_the_question_up() {
    let nodes = resolved(DOCUMENT);
    assert_eq!(at(&nodes, "tracker/plain/inherits"), Mutability::Editable);
}

#[test]
fn a_document_that_says_nothing_is_a_document_to_read() {
    // The default is fixed, not editable. A document nobody marked is not one to be typed into
    // by accident.
    let nodes = resolved(DOCUMENT);
    assert_eq!(at(&nodes, "quiet/untouched"), Mutability::Fixed);
}

#[test]
fn an_unnamed_wrapper_is_still_somewhere_the_answer_can_be_written() {
    // Addresses skip an unnamed wrapper, which is right for naming and wrong for this: the
    // wrapper carries `guarded`, and the field inside it has to inherit from there rather than
    // from the tab two levels up.
    let nodes = resolved(DOCUMENT);
    assert_eq!(at(&nodes, "tracker/through_a_wrapper"), Mutability::Guarded);
}

#[test]
fn nothing_at_an_address_is_fixed_rather_than_an_error() {
    let nodes = resolved(DOCUMENT);
    assert_eq!(at(&nodes, "tracker/nothing_here"), Mutability::Fixed);
    assert_eq!(at(&nodes, ""), Mutability::Fixed);
}

#[test]
fn a_formula_decides_it_through_the_shadow_it_leaves() {
    // `mutable` may be computed, and what the rest of the document sees is the computed answer.
    // Reading the raw parameter would find the formula text and make nothing of it.
    let text = "\
tab t (label=\"T\", mutable=true) {
    bool editing = false
    div panel (mutable=$(editing ? \"true\" : \"guarded\")) {
        string field = \"\"
    }
}
";
    let nodes = resolved(text);
    assert_eq!(
        at(&nodes, "t/panel/field"),
        Mutability::Guarded,
        "the formula's answer was not the one used"
    );
}

#[test]
fn a_word_nobody_recognises_inherits_rather_than_freezing() {
    // A typo should pass the question up, not silently make a subtree read-only.
    let text = "\
tab t (label=\"T\", mutable=true) {
    div panel (mutable=\"gaurded\") {
        string field = \"\"
    }
}
";
    let nodes = resolved(text);
    assert_eq!(at(&nodes, "t/panel/field"), Mutability::Editable);
}

#[test]
fn the_real_food_tracker_says_its_day_is_the_viewers() {
    // The case this exists for, read from the document itself rather than a fixture: the day
    // being looked at is marked guarded, while the meals recorded against it are not.
    let path = std::path::Path::new(
        "E:/Source/Repos/Secrebot-Docs/personal-stats/documents/tracker_v2.os",
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("the deployed tracker is not beside this checkout; skipping");
        return;
    };
    overseer::docmgr::manager::DocumentManager::set_current_document(Some(
        path.to_string_lossy().as_ref(),
    ));
    let nodes = resolved(&text);
    assert!(
        is_view_state(&nodes, "tracker_v2/Selected/selected_date"),
        "the selected day is not being read as the viewer's"
    );
    overseer::docmgr::manager::DocumentManager::set_current_document(None);
}
