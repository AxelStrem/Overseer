//! Prose must not destroy the document it is written into.
//!
//! A diary entry is a paragraph, and a paragraph has quotation marks and line breaks in it.
//! Written verbatim, the first quotation mark ended the value and the rest of the sentence
//! became stray tokens in the middle of a list; a newline spread one value over several lines
//! and the next field landed inside it. Either way the entry was lost, and the damage showed
//! up as a document that no longer parsed - nowhere near the text that caused it.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::{OverseerNode, OverseerValue};

/// A document holding one string, and what comes back after a save and a reload.
fn round_trip(value: &str) -> String {
    let escaped_for_source = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    let text = format!(
        "tab t (label=\"T\", mutable=true) {{
    string note = \"{escaped_for_source}\"
}}
"
    );
    let nodes = app_api::load_document(text).expect("load");
    let saved = OverseerFileHandler::serialize_nodes(&nodes).expect("save");
    let again = app_api::load_document(saved).expect("the saved document no longer parses");
    match find(&again, "note") {
        Some(OverseerValue::String(s)) => s,
        other => panic!("the value came back as {other:?}"),
    }
}

fn find(nodes: &[OverseerNode], name: &str) -> Option<OverseerValue> {
    for n in nodes {
        if n.name == name {
            return n
                .parameters
                .get("_computed_value")
                .or(n.parameters.get("value"))
                .cloned();
        }
        if let Some(v) = find(&n.children, name) {
            return Some(v);
        }
    }
    None
}

#[test]
fn a_quotation_mark_survives() {
    let prose = "They said \"take it easy\" and I did.";
    assert_eq!(round_trip(prose), prose);
}

#[test]
fn a_line_break_survives() {
    let prose = "Forty points today.\n\nWell above the usual twenty-five.";
    assert_eq!(round_trip(prose), prose);
}

#[test]
fn a_backslash_survives() {
    assert_eq!(round_trip("a \\ b"), "a \\ b");
}

#[test]
fn a_whole_paragraph_of_it_survives() {
    let prose = "Busy day: 40 points against a usual 25, \"mostly\" in the evening.\n\
                 Ate late - 2,100 kcal, and a 20% shortfall on protein.\tBought milk.";
    assert_eq!(round_trip(prose), prose);
}

#[test]
fn the_document_around_it_is_still_intact() {
    // The real failure was structural: what followed the prose was swallowed by it, and the
    // list came back short. Written into a list entry, as the diary will write it.
    let text = "tab t (label=\"T\", mutable=true) {
    div (hidden=true) {
        div Day (layout=\"vertical\", margin=0) {
            date day = \"2026-01-01\"
            string recap = \"\"
        }
    }

    list Days (entry=<Day>, key=\"day\", layout=\"vertical\") {
        - {
            - day = \"2026-08-12\"
            - recap = \"A \\\"good\\\" day.\nTwo paragraphs of it.\"
        }
        - {
            - day = \"2026-08-13\"
            - recap = \"The day after.\"
        }
    }
}
";
    let nodes = app_api::load_document(text.to_string()).expect("load");
    let saved = OverseerFileHandler::serialize_nodes(&nodes).expect("save");
    let again = app_api::load_document(saved.clone()).expect("the document no longer parses");

    fn list<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if let Some(f) = list(&n.children, name) {
                return Some(f);
            }
        }
        None
    }
    let days = list(&again, "Days").expect("the list is gone");
    assert_eq!(
        days.children.len(),
        2,
        "the entry after the prose was swallowed by it:
{saved}"
    );
    assert_eq!(
        find(std::slice::from_ref(&days.children[0]), "recap"),
        Some(OverseerValue::String("A \"good\" day.
Two paragraphs of it.".into())),
        "the prose came back changed:
{saved}"
    );
    assert_eq!(
        find(std::slice::from_ref(&days.children[1]), "recap"),
        Some(OverseerValue::String("The day after.".into())),
        "the entry after the prose was damaged:
{saved}"
    );
}
