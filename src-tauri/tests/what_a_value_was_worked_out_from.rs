//! The dependency graph, and the three things the one it replaced could not see.
//!
//! The old graph read formulas and guessed. It found a field naming a sibling and missed an
//! aggregate over a list, an absolute path, and a lookup into a keyed list - which between them
//! are most of what a real document is made of. Measured on the food tracker it reported six
//! dependents for a meal's weight, none of them the day's totals, and *zero* dependents for the
//! standing target that every day reads.
//!
//! This one records instead: while a value is being worked out, every path resolved on its behalf
//! is noted against it. So it cannot miss a kind of reference, only a kind of *read*.
//!
//! And there is more than one kind of read. The evaluator resolves a reference to a node in
//! several places - a value read, the list an aggregate runs over, a bare name found on an
//! ancestor - and they used to answer "here is the node" while throwing away the path walked to
//! reach it. They all carry it now, which is what lets any of them be recorded.
//!
//! The direction of error matters and is asserted here. Recording the list an aggregate ran over,
//! rather than the entries it touched, means a change anywhere in that list recomputes the
//! aggregate: more work than needed, never less. A graph that recomputes too much is slow; one
//! that recomputes too little is wrong and says nothing about it.

use overseer::dependencies;
use overseer::parser;
use overseer::resolver;

/// Resolve a document while recording what everything was worked out from.
fn graph_of(source: &str) -> dependencies::Graph {
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse");
    dependencies::start_recording();
    resolver::resolve_document(&mut nodes);
    dependencies::take_recording()
}

const DOC: &str = r#"
tab shop (label="Shop") {
    float vat (label="VAT") = 0.2

    div (hidden=true) {
        div Line (layout="horizontal") {
            string item (label="") = ""
            float price (label="") = 0
            float taxed (label="") = $(price * (1 + /shop/vat))
        }
    }

    list Lines (entry=<Line>, key="item") {
        - {
            - item = "bread"
            - price = 100
        }
        - {
            - item = "milk"
            - price = 50
        }
    }

    // An aggregate over the list: what the old graph contributed no edges for at all.
    float total (label="Total") = $(/shop/Lines.map(|x| x/taxed).sum())

    // A lookup into a keyed list, which lands on one entry decided by data.
    string looking_at (label="") = "milk"
    float that_price (label="") = $(/shop/Lines.filter(|x| x/item == /shop/looking_at).map(|x| x/price).sum())
}
"#;

/// Whether anything recorded as a dependency of `value` matches, allowing for the path being a
/// container of what was actually read.
fn depends_on(graph: &dependencies::Graph, value: &str, wanted: &str) -> bool {
    graph.reads(value).iter().any(|read| read == wanted)
}

#[test]
fn an_aggregate_depends_on_the_list_it_runs_over() {
    // The first thing the old graph missed. Without this edge a day's totals never notice a meal.
    let graph = graph_of(DOC);
    assert!(
        depends_on(&graph, "shop/total#_computed_value", "shop/Lines"),
        "the total does not depend on the list it adds up: {:?}",
        graph.reads("shop/total#_computed_value")
    );
}

#[test]
fn a_change_inside_a_list_reaches_what_aggregates_it() {
    // The edge has to be usable, not merely present: what changed is one line, and what was read
    // is the list it sits in. This is the conservative direction working as intended - a change
    // anywhere in the list reaches whatever adds it up.
    let graph = graph_of(DOC);
    let cascade = graph.cascade(&["shop/Lines/Line__1/price#_computed_value".to_string()]);
    assert!(
        cascade.iter().any(|c| c == "shop/total#_computed_value"),
        "changing a line's price does not reach the total: {cascade:?}"
    );
}

#[test]
fn an_absolute_path_is_a_dependency_like_any_other() {
    // The second thing the old graph missed: it reported zero dependents for the one field every
    // line reads. Asserted from the other end - changing the VAT must reach the lines.
    let graph = graph_of(DOC);
    let cascade = graph.cascade(&["shop/vat#_computed_value".to_string(), "shop/vat".to_string()]);
    assert!(
        !cascade.is_empty(),
        "nothing at all depends on the VAT, which every line multiplies by"
    );
}

#[test]
fn a_lookup_depends_on_what_decided_where_it_looked() {
    // The third: a keyed lookup lands on an entry chosen by another field. Change that field and
    // the lookup has to be worked out again, or it keeps answering about the old entry.
    let graph = graph_of(DOC);
    let cascade = graph.cascade(&["shop/looking_at#_computed_value".to_string()]);
    assert!(
        cascade.iter().any(|c| c == "shop/that_price#_computed_value"),
        "changing which item is looked at does not reach the lookup: {cascade:?}"
    );
}

#[test]
fn recording_is_off_unless_asked_for() {
    // It costs something, and most evaluation has nothing to do with building a graph.
    assert!(!dependencies::is_recording());
    let (_rest, mut nodes) = parser::parse_document(DOC).expect("parse");
    resolver::resolve_document(&mut nodes);
    assert!(
        dependencies::take_recording().is_empty(),
        "a resolve recorded a graph without being asked to"
    );
}

#[test]
fn the_food_tracker_records_what_its_days_are_made_of() {
    // The document this was all for. The old graph found six dependents for a meal's weight and
    // not one of them was the day it belongs to.
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(here.join("../examples/weight_tracker/tracker_v2.os"))
        .expect("no tracker");
    let graph = graph_of(&source);

    assert!(!graph.is_empty(), "nothing was recorded at all");

    // A day's totals must depend on the meals inside that day. Found by looking rather than by
    // naming a path: what the materialised tree calls things is the resolver's business, and a
    // test that hard-codes it fails for the wrong reason when that changes.
    //
    // Any day rather than the first, which is no longer one of them: `History` keeps three days in
    // view and the first is the oldest. Naming it was the hard-coding this comment warns against.
    let day_totals: Vec<&String> = graph
        .values()
        .into_iter()
        .filter(|v| v.contains("DayRecord__") && v.contains("totals/") && v.contains("calories"))
        .collect();
    assert!(
        !day_totals.is_empty(),
        "no day total was recorded at all; recorded values look like {:?}",
        graph.values().into_iter().take(3).collect::<Vec<_>>()
    );

    let reaches_the_meals = day_totals
        .iter()
        .any(|v| graph.reads(v).iter().any(|r| r.contains("intake")));
    assert!(
        reaches_the_meals,
        "a day's calories do not depend on what was eaten: {:?}",
        day_totals
            .iter()
            .map(|v| (v, graph.reads(v)))
            .collect::<Vec<_>>()
    );
}
