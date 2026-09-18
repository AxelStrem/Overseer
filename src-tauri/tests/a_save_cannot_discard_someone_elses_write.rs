//! A page saves the text it holds. If the file moved on, that text was built without whatever
//! moved it, and writing it throws the other write away.
//!
//! The situation is ordinary here: a document is open in a browser tab while the bot logs a
//! meal into it. The tab has been holding its own text since it loaded, so pressing Save writes
//! a document that never had the meal in it. Nothing warns either party; the meal is simply
//! gone, and the figures that depended on it are gone with it.
//!
//! A check for this existed and was correct - and then the page stopped using the command that
//! carried it. Sending the text already in hand is far cheaper than uploading the document to
//! be serialized, so every save moved to `save_overseer_file_from_text`, where nothing was
//! checked. The guard was not removed; it was walked around.
//!
//! What these pin is that all three ways of saving ask the same question, and that not asking
//! is only possible by saying nothing about what you started from - which is what saving a
//! document for the first time looks like.
//!
//! The comparison is made after canonicalizing, but that absorbs less than it sounds: the
//! serializer's whole job is to give back what it was given, so spacing and comments survive
//! it. In practice this asks whether the file changed at all - which is the right question,
//! because the page is comparing against the exact bytes it loaded.

use overseer::app_api;

const DOCUMENT: &str = "\
tab log (label=\"Log\", mutable=true) {
    list Entries (entry=<Row>, key=\"at\") {
        - { - at = \"2026-09-19T08:00:00+04:00\" - what = \"coffee\" }
    }
    div (hidden=true) {
        div Row (layout=\"horizontal\") {
            string at = \"\"
            string what = \"\"
        }
    }
}
";

/// The same document after somebody else appended to it.
const AFTER_SOMEONE_ELSE_WROTE: &str = "\
tab log (label=\"Log\", mutable=true) {
    list Entries (entry=<Row>, key=\"at\") {
        - { - at = \"2026-09-19T08:00:00+04:00\" - what = \"coffee\" }
        - { - at = \"2026-09-19T13:00:00+04:00\" - what = \"lunch\" }
    }
    div (hidden=true) {
        div Row (layout=\"horizontal\") {
            string at = \"\"
            string what = \"\"
        }
    }
}
";

#[test]
fn a_file_that_has_not_moved_is_saved() {
    assert!(app_api::still_says_what_it_did(DOCUMENT, Some(DOCUMENT)));
}

#[test]
fn a_file_that_has_moved_on_is_refused() {
    // The case that loses work: the page holds the document as it was, the file holds lunch.
    assert!(!app_api::still_says_what_it_did(AFTER_SOMEONE_ELSE_WROTE, Some(DOCUMENT)));
}

#[test]
fn a_caller_that_says_nothing_is_not_checked() {
    // A document saved for the first time has nothing to have moved on from, and a caller that
    // does not say what it started from gets the old behaviour rather than a refusal.
    assert!(app_api::still_says_what_it_did(AFTER_SOMEONE_ELSE_WROTE, None));
}

#[test]
fn a_hand_edit_that_only_moves_whitespace_is_still_a_conflict() {
    // Stated rather than left to be discovered: spacing survives a save, so changing it is a
    // change. That is the right way round - the page loaded the exact bytes it is checking
    // against, so the only way to see a difference here is that somebody edited the file.
    let spaced = DOCUMENT.replace("mutable=true) {", "mutable=true)   {");
    assert_ne!(spaced, DOCUMENT);
    assert!(!app_api::still_says_what_it_did(&spaced, Some(DOCUMENT)));
}

#[test]
fn a_comment_added_by_hand_is_a_conflict() {
    // Comments survive canonicalizing - that is the point of the serializer - so a hand edit
    // that only adds one is still a write this page would be discarding.
    let commented = format!("// written by hand while the page was open
{}", DOCUMENT);
    assert!(!app_api::still_says_what_it_did(&commented, Some(DOCUMENT)));
}
