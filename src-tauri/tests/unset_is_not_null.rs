//! When a field reads its fallback, and what it lands on when the fallback has nothing to say.
//!
//! A field is *unset* when it has no value or its value is written as null, and only an unset
//! field consults its `fallback`. A field that states a formula is set - even a formula that works
//! out to null - because what a formula evaluates to is not the question. Asking it that way would
//! make "does this field depend on its fallback" an answer you could only get by evaluating, and
//! the dependency has to be knowable before that.
//!
//! `default` is where an unset field lands when its fallback cannot answer. The case it exists
//! for is a pair that fall back to each other - state the portions and the grams follow, state the
//! grams and the portions do - where neither is stated, so each one's fallback asks the other and
//! neither can say. A default depends on nothing, which is what makes it somewhere for that to
//! stop.

use overseer::app_api;
use overseer::types::{OverseerNode, OverseerValue};

fn read(nodes: &[OverseerNode], entry_index: usize, field: &str) -> OverseerValue {
    fn list_named<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if let Some(f) = list_named(&n.children, name) {
                return Some(f);
            }
        }
        None
    }
    fn child<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
        for c in &node.children {
            if c.name == name {
                return Some(c);
            }
            if let Some(f) = child(c, name) {
                return Some(f);
            }
        }
        None
    }
    let list = list_named(nodes, "Meals").expect("no Meals list");
    let entry = &list.children[entry_index];
    let node = child(entry, field).unwrap_or_else(|| panic!("no `{field}` on entry {entry_index}"));
    // What a reader sees, through the same call the renderer and the formulas use - an unset
    // field's value is not in its `value` parameter, which is the whole subject here.
    overseer::formula_evaluator::FormulaEvaluator::get_effective_value_for_node(
        node,
        &["t".to_string(), "Meals".to_string(), entry.name.clone()],
        nodes,
    )
    .unwrap_or(OverseerValue::Null)
}

fn number(value: OverseerValue) -> f64 {
    match value {
        OverseerValue::Integer(i) => i as f64,
        OverseerValue::Float(f) => f,
        other => panic!("not a number: {other:?}"),
    }
}

const DOC: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Meal (layout="horizontal") {
            float portion_weight (hidden=true) = 50

            // Either side may be stated; the other follows. Neither stated, and `portions` is one.
            float portions (label="", fallback=$(grams / portion_weight), default=1) = null
            float grams (label="", fallback=$(portions * portion_weight)) = null

            // Stated as a formula that works out to null. Being stated is what matters, so this
            // must not reach for the fallback beside it.
            float from_formula (label="", fallback=$(999)) = $(portions > 1000 ? 1 : null)
        }
    }

    list Meals (entry=<Meal>) {
        - {
            - portions = 3
        }
        - {
            - grams = 200
        }
        - { }
    }
}
"#;

#[test]
fn a_stated_value_is_read_and_the_other_side_follows() {
    let nodes = app_api::load_document(DOC.to_string()).expect("load");

    assert_eq!(number(read(&nodes, 0, "portions")), 3.0);
    assert_eq!(
        number(read(&nodes, 0, "grams")),
        150.0,
        "grams should follow from three portions of fifty"
    );

    assert_eq!(number(read(&nodes, 1, "grams")), 200.0);
    assert_eq!(
        number(read(&nodes, 1, "portions")),
        4.0,
        "portions should follow from two hundred grams"
    );
}

#[test]
fn neither_stated_lands_on_the_default() {
    // The case that had no graceful answer: each fallback asks the other and neither can say.
    // Before there was a default, this read as an error that alternated with null and kept the
    // whole document from ever settling.
    let nodes = app_api::load_document(DOC.to_string()).expect("load");

    assert_eq!(
        number(read(&nodes, 2, "portions")),
        1.0,
        "with neither side stated, portions should land on its default"
    );
    assert_eq!(
        number(read(&nodes, 2, "grams")),
        50.0,
        "and grams should then follow from it"
    );
}

#[test]
fn a_field_that_states_a_formula_is_set_even_when_it_answers_null() {
    // The distinction the whole rule turns on. `from_formula` works out to null here, and a
    // fallback of 999 sits beside it: reading 999 would mean the rule was asking what the field
    // evaluated to rather than what it states.
    let nodes = app_api::load_document(DOC.to_string()).expect("load");

    for entry in 0..3 {
        let held = read(&nodes, entry, "from_formula");
        let took_the_fallback = match &held {
            OverseerValue::Integer(i) => *i == 999,
            OverseerValue::Float(f) => *f == 999.0,
            _ => false,
        };
        assert!(
            !took_the_fallback,
            "entry {entry} took its fallback despite stating a formula: {held:?}"
        );
    }
}

#[test]
fn the_rule_is_answerable_without_evaluating_anything() {
    // What makes a fallback dependency knowable ahead of time: the answer comes from the document
    // as written, so it is the same before any pass has run and after all of them.
    let (_rest, parsed) = overseer::parser::parse_document(DOC).expect("parse");

    fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if let Some(f) = find(&n.children, name) {
                return Some(f);
            }
        }
        None
    }

    let unparsed_portions = find(&parsed, "portions").expect("no portions in the template");
    assert!(
        !overseer::formula_evaluator::FormulaEvaluator::states_a_value(
            &unparsed_portions.parameters
        ),
        "a field written as null should read as unset before anything is evaluated"
    );

    let unparsed_formula = find(&parsed, "from_formula").expect("no from_formula");
    assert!(
        overseer::formula_evaluator::FormulaEvaluator::states_a_value(&unparsed_formula.parameters),
        "a field written as a formula should read as set before anything is evaluated"
    );
}
