//! `random(seed, ..., n)`: a number a formula can draw and still give the same answer on every
//! pass. A board, a quote of the day or a shuffle is drawn afresh only when its seed changes.

use overseer::app_api;
use overseer::formula_evaluator::{EvaluationContext, FormulaEvaluator};
use overseer::types::{OverseerNode, OverseerValue};

/// Resolve a document holding one formula field, and read what it came to.
fn resolve(formula: &str) -> OverseerValue {
    let text = format!(
        "tab t (label=\"T\") {{
    string word (hidden=true) = \"board\"
    timestamp seed (hidden=true) = \"2026-10-29T10:00:00Z\"
    int out = $({formula})
}}
"
    );
    let nodes = app_api::load_document(text).expect("load");
    find(&nodes, "out").expect("no out field")
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

fn int(v: OverseerValue) -> i64 {
    match v {
        OverseerValue::Integer(i) => i,
        other => panic!("not a whole number: {other:?}"),
    }
}

/// Evaluate a formula against an empty document, for when a test draws thousands of times.
fn draw(formula: &str) -> i64 {
    let root = vec![OverseerNode::new_with_type("tab".to_string(), Some("t".to_string()))];
    let context = EvaluationContext::new(vec!["t".to_string()], &root);
    int(FormulaEvaluator::evaluate_formula(formula, &context).expect(formula))
}

#[test]
fn the_same_seed_gives_the_same_number_every_time() {
    let first = int(resolve("random(seed, 1000000)"));
    for _ in 0..3 {
        assert_eq!(int(resolve("random(seed, 1000000)")), first);
    }
}

#[test]
fn the_number_is_pinned_so_a_change_of_hash_shows() {
    // A document drawn today must draw the same after an upgrade: a saved seed is a saved board.
    assert_eq!(draw("random(\"minesweeper\", 7, 1000000)"), PINNED);
}

const PINNED: i64 = 933913;

#[test]
fn different_seeds_give_different_numbers() {
    let seen: std::collections::HashSet<i64> =
        (0..50).map(|i| draw(&format!("random(\"s\", {i}, 1000000000)"))).collect();
    assert!(seen.len() >= 49, "50 seeds gave only {} numbers", seen.len());
}

#[test]
fn every_part_of_the_seed_counts_and_so_do_their_boundaries() {
    let n = 1_000_000_000;
    let pairs = [
        (format!("random(\"ab\", \"c\", {n})"), format!("random(\"a\", \"bc\", {n})")),
        (format!("random(1, {n})"), format!("random(\"1\", {n})")),
        (format!("random(1, 2, {n})"), format!("random(2, 1, {n})")),
        (format!("random(\"x\", {n})"), format!("random(\"x\", 0, {n})")),
    ];
    for (a, b) in pairs {
        assert_ne!(draw(&a), draw(&b), "{a} and {b} drew the same");
    }
}

#[test]
fn a_whole_number_from_arithmetic_is_the_same_seed_as_one_written_out() {
    assert_eq!(draw("random(\"c\", 2 + 1, 1000000)"), draw("random(\"c\", 3, 1000000)"));
}

#[test]
fn a_seed_read_from_fields_is_the_text_they_hold() {
    // A timestamp field and its text written out are one seed, so a document draws the same
    // whichever way a formula reaches the value.
    let a = int(resolve("random(seed, word, 1000000)"));
    let b = int(resolve("random(\"2026-10-29T10:00:00Z\", \"board\", 1000000)"));
    assert_eq!(a, b);
    assert_ne!(a, int(resolve("random(seed, 1000000)")), "every field read is part of the seed");
}

#[test]
fn draws_spread_evenly_and_stay_below_n() {
    let mut buckets = [0usize; 10];
    for i in 0..10_000 {
        let k = draw(&format!("random(\"s\", {i}, 10)"));
        assert!((0..10).contains(&k), "random drew {k} for n = 10");
        buckets[k as usize] += 1;
    }
    for (k, count) in buckets.iter().enumerate() {
        assert!((900..=1100).contains(count), "bucket {k} got {count} of 10 000: {buckets:?}");
    }
}

#[test]
fn n_must_be_a_whole_number_above_zero() {
    for formula in ["random(seed, 0)", "random(seed, -3)", "random(seed, 2.5)", "random(seed)", "random(10)"] {
        let text = format!("{:?}", resolve(formula));
        assert!(text.to_lowercase().contains("error"), "{formula} gave {text}");
    }
}
