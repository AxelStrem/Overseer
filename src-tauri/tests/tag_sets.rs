//! A `tags` node is a set of values, and every list method already knows what to do with one.

use overseer::formula_evaluator::FormulaEvaluator;
use overseer::parser;
use overseer::resolver;
use overseer::types::OverseerValue;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    string selected = "edeka"

    tags shops = "lidl, edeka ,ikea"
    tags single = "lidl"
    tags none = ""

    int stocked_here = $(shops.filter(|s| s == selected).count())
    int single_here = $(single.filter(|s| s == selected).count())
    int none_here = $(none.filter(|s| s == selected).count())
    int how_many = $(shops.count())
    int partial = $(shops.filter(|s| s == "lid").count())
}
"#;

fn value(name: &str) -> OverseerValue {
    let (_rest, mut nodes) = parser::parse_document(DOCUMENT).expect("parse");
    resolver::resolve_document(&mut nodes);
    let tab = nodes.iter().find(|n| n.name == "t").expect("tab");
    let node = tab.children.iter().find(|c| c.name == name).expect(name);
    FormulaEvaluator::get_effective_value_for_node(
        node,
        &["t".to_string(), name.to_string()],
        &nodes,
    )
    .expect("evaluate")
}

#[test]
fn a_tag_set_can_be_asked_whether_it_holds_something() {
    assert_eq!(value("stocked_here"), OverseerValue::Integer(1));
    assert_eq!(value("single_here"), OverseerValue::Integer(0));
}

#[test]
fn an_empty_tag_set_holds_nothing() {
    assert_eq!(value("none_here"), OverseerValue::Integer(0));
}

#[test]
fn spacing_around_a_tag_is_not_part_of_it() {
    // Written " edeka " in the document above, and matched against "edeka".
    assert_eq!(value("how_many"), OverseerValue::Integer(3));
    assert_eq!(value("stocked_here"), OverseerValue::Integer(1));
}

#[test]
fn a_tag_matches_whole_or_not_at_all() {
    // The reason this is a set rather than a string with commas in it: `lid` is not `lidl`,
    // and a shop called `lidl_express` must not be found by looking for `lidl`.
    assert_eq!(value("partial"), OverseerValue::Integer(0));
}
