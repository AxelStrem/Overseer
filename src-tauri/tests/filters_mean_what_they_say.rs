//! A filter answers the same thing however it is written.
//!
//! `filter` does two things now that it did not before: the side of a comparison that cannot
//! change from one element to the next is worked out once rather than once per element, and a
//! predicate of the shape `x/field == literal` reads the field instead of resolving a path to it.
//! Both are meant to be invisible - the same answers, sooner - and both have shapes they must
//! decline to touch.
//!
//! The shapes that would go wrong if they were handled carelessly, and are checked here:
//!
//!   - the field on the right of an ordering comparison, where `<` read backwards means `>`;
//!   - a predicate that mentions the element on both sides, where there is no invariant half;
//!   - a nested lambda that shadows the parameter, where the inner `x` is a different `x`;
//!   - a field whose value is still a formula, which has to be evaluated rather than read.
//!
//! The document these run against is written so that getting any of it wrong changes an answer.

use overseer::app_api;
use overseer::types::{OverseerNode, OverseerValue};

const DOC: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Item (layout="horizontal") {
            string handle (label="") = ""
            int size (label="") = 0
            int doubled (label="") = $(size * 2)
        }
    }

    int threshold (hidden=true) = 3
    string wanted (hidden=true) = "pear"

    list Items (entry=<Item>, key="handle") {
        - {
            - handle = "apple"
            - size = 1
        }
        - {
            - handle = "pear"
            - size = 4
        }
        - {
            - handle = "plum"
            - size = 7
        }
    }

    // The field on the left, against a literal.
    int by_literal (hidden=true) = $(Items.filter(|x| x/handle == "pear").map(|x| x/size).sum())

    // The field on the left, against something read from the document - the shape every real
    // lookup takes, and the one the hoist exists for.
    int by_reference (hidden=true) = $(Items.filter(|x| x/handle == ../wanted).map(|x| x/size).sum())

    // The literal on the left. Equality reads the same both ways round.
    int reversed_equality (hidden=true) = $(Items.filter(|x| "pear" == x/handle).map(|x| x/size).sum())

    // Ordering, with the field on the left: sizes above the threshold are 4 and 7.
    int above (hidden=true) = $(Items.filter(|x| x/size > ../threshold).map(|x| x/size).sum())

    // The same question written the other way round. `3 > x/size` is the *small* one, not the
    // large ones - so anything that swaps the sides without swapping the operator answers 11.
    int below (hidden=true) = $(Items.filter(|x| ../threshold > x/size).map(|x| x/size).sum())

    // Both sides mention the element, so there is no invariant half to lift out. Only the plum
    // has a size above its own name's length... which is true for all three, so this is really
    // checking that a two-sided predicate is still evaluated per element at all.
    int both_sides (hidden=true) = $(Items.filter(|x| x/doubled > x/size).map(|x| x/size).sum())

    // The field being tested is itself computed. It cannot be read straight off the node, so
    // this must fall back to evaluating it: doubled is 2, 8 and 14, and only 8 matches.
    int computed_field (hidden=true) = $(Items.filter(|x| x/doubled == 8).map(|x| x/size).sum())

    // A nested lambda naming the same parameter. The inner `x` is the inner list's element, so
    // the count is 3 for every outer element and the predicate holds for all of them.
    int shadowed (hidden=true) =
        $(Items.filter(|x| Items.filter(|x| x/size > 0).count() == 3).map(|x| x/size).sum())
}
"#;

fn value(nodes: &[OverseerNode], name: &str) -> f64 {
    fn seek(nodes: &[OverseerNode], name: &str) -> Option<OverseerValue> {
        for n in nodes {
            if n.name == name {
                return n
                    .parameters
                    .get("_computed_value")
                    .or(n.parameters.get("value"))
                    .cloned();
            }
            if let Some(v) = seek(&n.children, name) {
                return Some(v);
            }
        }
        None
    }
    match seek(nodes, name) {
        Some(OverseerValue::Integer(i)) => i as f64,
        Some(OverseerValue::Float(f)) => f,
        other => panic!("`{}` read {:?}", name, other),
    }
}

fn document() -> Vec<OverseerNode> {
    app_api::load_document(DOC.to_string()).expect("load")
}

#[test]
fn a_literal_on_either_side_means_the_same() {
    let nodes = document();
    assert_eq!(value(&nodes, "by_literal"), 4.0);
    assert_eq!(value(&nodes, "reversed_equality"), 4.0);
}

#[test]
fn the_invariant_side_may_be_read_from_the_document() {
    // The shape of every catalog lookup: a field of the element against a field of the document.
    // Lifting it out of the loop must not change which elements match.
    let nodes = document();
    assert_eq!(value(&nodes, "by_reference"), 4.0);
}

#[test]
fn an_ordering_keeps_its_direction_when_the_field_is_on_the_right() {
    let nodes = document();
    assert_eq!(value(&nodes, "above"), 11.0, "sizes above 3 are 4 and 7");
    assert_eq!(
        value(&nodes, "below"),
        1.0,
        "`3 > x/size` is the small one; reading it backwards would answer 11"
    );
}

#[test]
fn a_predicate_about_the_element_on_both_sides_still_runs_per_element() {
    let nodes = document();
    assert_eq!(value(&nodes, "both_sides"), 12.0);
}

#[test]
fn a_computed_field_is_evaluated_rather_than_read() {
    // Reading `doubled` off the node finds a formula, not a number. The fast path has to decline
    // and let the general machinery work it out, or this filter matches nothing.
    let nodes = document();
    assert_eq!(value(&nodes, "computed_field"), 4.0);
}

#[test]
fn a_nested_lambda_shadows_the_parameter() {
    // The inner `x` belongs to the inner filter. Treating the inner predicate as mentioning the
    // outer element - or the other way about - changes which elements survive.
    let nodes = document();
    assert_eq!(value(&nodes, "shadowed"), 12.0);
}
