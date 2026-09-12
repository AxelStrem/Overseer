//! What is left over.
//!
//! The formula language had `+ - * /` and nothing to ask "every third" or "which of these two"
//! with. `%` fills that in: it binds like multiplication, keeps the sign of its left operand the
//! way C and Rust do, and refuses zero the way division already did.
//!
//! Worth saying what it is *not* for, since that is what prompted it. Striping a list by row
//! needs a row's position on screen, and a formula cannot see one: formulas are evaluated in
//! document order, and the sort that decides what the screen shows is applied afterwards by the
//! renderer. `%` makes "every nth" expressible over data - the seventh of the month, one of
//! three states - not over position.

use overseer::app_api;
use overseer::types::{OverseerNode, OverseerValue};

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

fn eval(formula: &str) -> OverseerValue {
    let text = format!("tab t (label=\"T\") {{\n    float out = $({formula})\n}}\n");
    let nodes = app_api::load_document(text).expect("load");
    find(&nodes, "out").expect("no out field")
}

fn number(formula: &str) -> f64 {
    match eval(formula) {
        OverseerValue::Float(f) => f,
        OverseerValue::Integer(i) => i as f64,
        other => panic!("`{}` gave {:?}", formula, other),
    }
}

#[test]
fn it_leaves_the_remainder() {
    assert_eq!(number("7 % 3"), 1.0);
    assert_eq!(number("10 % 5"), 0.0);
    assert_eq!(number("2 % 5"), 2.0);
}

#[test]
fn it_binds_as_tightly_as_multiplication() {
    // `1 + 7 % 3` is `1 + (7 % 3)`, not `(1 + 7) % 3`, which would be 2 either way - so the
    // cases that tell the difference are the ones worth writing.
    assert_eq!(number("1 + 7 % 3"), 2.0);
    assert_eq!(number("10 - 7 % 3"), 9.0);
    assert_eq!(number("(1 + 7) % 3"), 2.0);
    // Same precedence as `*`, applied left to right.
    assert_eq!(number("7 % 3 * 2"), 2.0);
    assert_eq!(number("2 * 7 % 3"), 2.0);
}

#[test]
fn it_keeps_the_sign_of_the_left_operand() {
    // C's answer rather than mathematics'. Written down because the other convention would give
    // 2 here, and picking one of three things by `n % 3` quietly breaks on a negative n.
    assert_eq!(number("0 - 1 % 3"), -1.0);
    assert_eq!(number("(0 - 7) % 3"), -1.0);
    assert_eq!(number("7 % (0 - 3)"), 1.0);
}

#[test]
fn it_works_on_fractions_too() {
    assert_eq!(number("7.5 % 2"), 1.5);
}

#[test]
fn zero_is_refused_rather_than_answered() {
    // Division by zero is already a hard error, and the two should agree: an answer of nought or
    // a NaN spreading through a document would be worse than a row that says it is broken.
    let reading = eval("7 % 0");
    assert!(
        matches!(&reading, OverseerValue::String(s) if s.contains("invalid")),
        "7 % 0 gave {:?}",
        reading,
    );
}

#[test]
fn it_answers_the_question_it_was_added_for() {
    // Every nth of something real. `day_of_month` is one of the three calendar functions the
    // scheduler uses, and until now the arithmetic after it could not be written.
    let text = "tab t (label=\"T\") {\n    \
                timestamp when (hidden=true) = \"2026-09-21T09:00:00Z\"\n    \
                int nth = $(day_of_month(../when) % 7)\n}\n";
    let nodes = app_api::load_document(text.to_string()).expect("load");
    let value = find(&nodes, "nth").expect("no nth field");
    let got = match value {
        OverseerValue::Float(f) => f,
        OverseerValue::Integer(i) => i as f64,
        other => panic!("{:?}", other),
    };
    assert_eq!(got, 0.0, "the 21st is a third week to the day");
}
