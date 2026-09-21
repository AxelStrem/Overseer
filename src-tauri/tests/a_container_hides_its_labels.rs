//! A container says once that the fields inside it do not show their labels.
//!
//! Said on every field instead, `label=""` is the same instruction repeated - and repeated in
//! the one place a table already answers it, since a heading that names the column makes every
//! cell below it say the name again. `hide-labels` moves the sentence to the container that
//! knows why: this list is a table, this group's fields are obvious from where they sit.
//!
//! It travels the way a colour does, copied down onto every descendant by the resolver, which
//! is what makes "unless one of them says otherwise" fall out for free - the copy stops where a
//! node states its own. What the copy must not do is reach the file: it was never written
//! there, and the marker that says so is the same one that once emptied a coloured template.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(hit) = find(&node.children, name) {
            return Some(hit);
        }
    }
    None
}

fn resolved(source: &str) -> Vec<OverseerNode> {
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse");
    resolver::resolve_document(&mut nodes);
    nodes
}

/// Whether this field would draw its label - the question the renderer asks of the node alone.
fn hides_its_label(nodes: &[OverseerNode], name: &str) -> bool {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    // Both shapes, because how it was written decides which arrives - the same two the
    // renderer accepts.
    match node.parameters.get("hide-labels") {
        Some(OverseerValue::Boolean(said)) => *said,
        Some(OverseerValue::String(said)) => said == "true",
        _ => false,
    }
}

const DOC: &str = r#"
tab t (label="T") {
    string outside (label="outside") = ""

    div quiet (hide-labels=true) {
        string within (label="within") = ""

        div deeper {
            string further_in (label="further in") = ""
        }

        div loud (hide-labels=false) {
            string speaking_up (label="speaking up") = ""
        }
    }
}
"#;

#[test]
fn a_field_outside_keeps_its_label() {
    assert!(!hides_its_label(&resolved(DOC), "outside"));
}

#[test]
fn a_field_inside_the_container_hides_it() {
    assert!(hides_its_label(&resolved(DOC), "within"));
}

#[test]
fn so_does_one_further_down() {
    // The point of saying it on the container: a group nested inside inherits the instruction
    // without having to repeat it.
    assert!(hides_its_label(&resolved(DOC), "further_in"));
}

#[test]
fn until_something_inside_says_otherwise() {
    // Nearest ancestor to state anything wins, which is a sentence CSS cannot say with a
    // descendant selector - hence doing this in the resolver rather than the stylesheet.
    assert!(!hides_its_label(&resolved(DOC), "speaking_up"));
}

// --- and none of it reaches the file ---

fn load_and_save(text: &str) -> String {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    overseer::source_registry::SourceRegistry::reset();
    let nodes = app_api::load_document(text.to_string()).expect("load");
    let across: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&nodes).expect("json")).expect("json");
    app_api::canonicalize_document(&OverseerFileHandler::serialize_nodes(&across).expect("save"))
}

#[test]
fn the_document_comes_back_as_it_was_written() {
    let text = DOC.trim_start();
    assert_eq!(load_and_save(text), text);
}

#[test]
fn the_instruction_is_not_copied_onto_every_field() {
    let text = DOC.trim_start();
    let saved = load_and_save(text);
    assert_eq!(
        saved.matches("hide-labels").count(),
        2,
        "the copies handed down to children were written to disk:\n{}",
        saved
    );
}

/// The trap this parameter walks into, and the reason the two lists became one.
///
/// A `_template_*` key on a node was read two ways: as "this parameter was handed down" and as
/// "this whole node came from a template, so write it without its contents". The serializer
/// kept a list of three exceptions for the first reading. A fourth inheritable parameter not on
/// that list means a container that hides its labels is written out empty.
#[test]
fn hiding_labels_on_a_template_does_not_empty_it() {
    // The shape that failed with a colour: a template holding a plain layout group, the group
    // holding an instance of a second template. The group came from no template of its own - it
    // only carried a handed-down parameter - and answering "did this come from a template" with
    // that marker wrote it out as `{}`.
    let text = "tab t (label=\"T\", mutable=true) {
    div (hidden=true) {
        div Scorer (layout=\"horizontal\", margin=0) {
            int input (hidden=true) = 0
            int out (label=\"Out\") = $(input * 2)
        }
        div Card (layout=\"vertical\", margin=0, hide-labels=true) {
            int amount = 1

            div (layout=\"horizontal\", margin=0) {
                string name (width=30%) = \"x\"

                <Scorer> quality {
                    - input = $(../../amount)
                }
            }
        }
    }

    list rows (entry=<Card>) {
        - {
            - amount = 3
        }
    }
}
";
    let saved = load_and_save(text);
    assert!(
        !saved.contains("div (layout=\"horizontal\", margin=0) {}"),
        "the group inside the template was written out empty:\n{}",
        saved
    );
    assert!(
        saved.contains("string name (width=30%)"),
        "the group's contents were dropped:\n{}",
        saved
    );
    assert_eq!(saved, text, "the document drifted on save");
}
