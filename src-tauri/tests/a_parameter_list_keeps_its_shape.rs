//! A parameter list written across several lines is still written across several lines.
//!
//! The serializer emits parameters on one line. A document authored with them wrapped - which is
//! how anything with more than three or four of them is readable - was therefore reformatted the
//! first time anything wrote to it, turning an edit to one field into a diff touching every node
//! that had wrapped. `tracker_v2.os` was checked byte for byte and so was safe; `diary.os`,
//! `shopping.os` and `tasks.os` were not, and they are the ones that have them.
//!
//! The live documents cannot be reached from here - they belong to the deployed repository - so
//! the shapes they use are copied below instead: a wrapped list header, a wrapped field, and a
//! wrapped field whose continuation is a formula.
//!
//! Checked through the path a save actually takes. Parsing and writing straight back is the
//! weaker question; what writes the file is parse, resolve, serialize, and the resolve is where a
//! node can be rebuilt without the text it came from.

use overseer::file_ops::OverseerFileHandler;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    int standing = 2000

    div Note (hidden=true, layout="horizontal") {
        timestamp at (label="", format="time", precision="minutes",
                      font-size=13px, width=10%) = "2026-01-01T00:00:00Z"
        string body (label="", width=90%) = ""
    }

    list Notes (entry=<Note>, key="at", layout="vertical", spacing=4,
                hidden=$(Notes.count() == 0)) {
        - {
            - at = "2026-09-18T09:00:00Z"
            - body = "a note"
        }
    }

    float target (label="Target kcal", precision=0,
                  fallback=$(/t/standing)) = null
}
"#;

/// Parse, resolve, write - what `save_document_from_text` does.
fn through_a_save(source: &str) -> String {
    let nodes = overseer::app_api::load_document(source.to_string()).expect("resolve");
    OverseerFileHandler::serialize_nodes(&nodes).expect("serialize")
}

#[test]
fn a_wrapped_parameter_list_is_not_flattened() {
    let written = through_a_save(DOCUMENT);
    let want: Vec<&str> = DOCUMENT.lines().collect();
    let got: Vec<&str> = written.lines().collect();
    if let Some(n) = want.iter().zip(got.iter()).position(|(a, b)| a != b) {
        panic!(
            "line {} was rewritten
  authored: {:?}
   written: {:?}",
            n + 1,
            want[n],
            got[n]
        );
    }
    assert_eq!(want.len(), got.len(), "the document changed length");
}

#[test]
fn writing_to_one_field_leaves_the_wrapped_ones_alone() {
    // The cost of the fault was never the shape itself - it was that one edit rewrote every
    // wrapped node, so a diff said nothing about what had actually changed.
    let mut nodes = overseer::app_api::load_document(DOCUMENT.to_string()).expect("resolve");
    overseer::actions::ActionExecutor::assign_value(
        &mut nodes,
        "/t/standing",
        overseer::types::OverseerValue::Integer(2500),
    )
    .expect("set");
    let written = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

    let changed: Vec<(usize, &str, &str)> = DOCUMENT
        .lines()
        .zip(written.lines())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(n, (a, b))| (n + 1, a, b))
        .collect();
    assert_eq!(
        changed.len(),
        1,
        "a write to one field rewrote {} lines: {:?}",
        changed.len(),
        changed
    );
    assert!(changed[0].2.contains("2500"), "the wrong line changed: {:?}", changed[0]);
}
