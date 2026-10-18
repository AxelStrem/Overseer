//! A container says once that the fields inside it do not show their labels.
//!
//! Said on every field instead, `label=""` is the same instruction repeated - and repeated in
//! the one place a table already answers it, since a heading that names the column makes every
//! cell below it say the name again. `context hide-labels = true` moves the sentence to the
//! container that knows why: this list is a table, this group's fields are obvious from where
//! they sit.
//!
//! It is a context, so a nearer container's says otherwise for what is inside that one, and a
//! field's own `hide-labels=` parameter is that field's alone. Nothing is copied onto the fields:
//! the page asks the field, then the nearest context, the way `nearest` does here. What the
//! resolver puts in front of a container's children must not reach the file either.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The ancestors of the field named `name`, nearest last, and the field itself.
fn path_to<'a>(nodes: &'a [OverseerNode], name: &str, above: &mut Vec<&'a OverseerNode>) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name && node.node_type != "context" {
            return Some(node);
        }
        above.push(node);
        if let Some(hit) = path_to(&node.children, name, above) {
            return Some(hit);
        }
        above.pop();
    }
    None
}

fn resolved(source: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse");
    resolver::resolve_document(&mut nodes);
    nodes
}

fn said(value: Option<&OverseerValue>) -> Option<bool> {
    match value? {
        OverseerValue::Boolean(said) => Some(*said),
        OverseerValue::String(said) => Some(said == "true"),
        _ => None,
    }
}

/// The context named `name` nearest the node: the copies the resolver put first among each
/// container's children, looked for from the node's own container outwards.
fn nearest(nodes: &[OverseerNode], above: &[&OverseerNode], name: &str) -> Option<bool> {
    let levels = above.iter().rev().map(|node| node.children.as_slice()).chain(std::iter::once(nodes));
    for children in levels {
        let copy = children
            .iter()
            .take_while(|child| child.node_type == "context")
            .chain(children.iter().rev().take_while(|child| child.node_type == "context"))
            .find(|child| child.name == name && child.parameters.contains_key("_context_copy"));
        if let Some(copy) = copy {
            return said(copy.parameters.get("_computed_value").or_else(|| copy.parameters.get("value")));
        }
    }
    None
}

/// Whether this field would draw its label - the question the renderer asks: its own parameter,
/// or else the nearest context.
fn hides_its_label(nodes: &[OverseerNode], name: &str) -> bool {
    let mut above = Vec::new();
    let node = path_to(nodes, name, &mut above).unwrap_or_else(|| panic!("no node named `{}`", name));
    said(node.parameters.get("hide-labels"))
        .or_else(|| nearest(nodes, &above, "hide-labels"))
        .unwrap_or(false)
}

const DOC: &str = r#"
tab t (label="T") {
    string outside (label="outside") = ""

    div quiet {
        context hide-labels = true
        string within (label="within") = ""
        string own (label="own", hide-labels=false) = ""

        div deeper {
            string further_in (label="further in") = ""
        }

        div loud {
            context hide-labels = false
            string speaking_up (label="speaking up") = ""
        }
    }

    div told (hide-labels=true) {
        string not_told (label="not told") = ""
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
    // The point of saying it on the container: a group nested inside sees it without having to
    // repeat it.
    assert!(hides_its_label(&resolved(DOC), "further_in"));
}

#[test]
fn until_something_inside_says_otherwise() {
    assert!(!hides_its_label(&resolved(DOC), "speaking_up"));
}

#[test]
fn a_fields_own_word_wins_for_that_field() {
    assert!(!hides_its_label(&resolved(DOC), "own"));
}

#[test]
fn a_containers_parameter_is_its_own_and_not_handed_down() {
    // `hide-labels=` on a div was once copied onto everything inside it. It is a context now, and
    // a parameter on a container means the container alone.
    let nodes = resolved(DOC);
    let mut above = Vec::new();
    let field = path_to(&nodes, "not_told", &mut above).expect("not_told");
    assert_eq!(field.parameters.get("hide-labels"), None);
    assert!(!hides_its_label(&nodes, "not_told"));
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
        4,
        "the copies handed down to children were written to disk:\n{}",
        saved
    );
}

/// The trap a parameter handed down once walked into, kept for the context that replaced it.
///
/// A `_template_*` key on a node was read two ways: as "this parameter was handed down" and as
/// "this whole node came from a template, so write it without its contents". A container in a
/// template that hid its labels was written out empty. A context declared there must neither
/// empty it nor be written into every entry.
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
        div Card (layout=\"vertical\", margin=0) {
            context hide-labels = true
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
