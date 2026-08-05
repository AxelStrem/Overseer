//! The food catalog document: `examples/weight_tracker/foods.os`.
//!
//! It is the source of nutritional data for the calorie tracker, so what matters is that a
//! food which states only its per-100 g figures still yields correct per-portion numbers,
//! that omitted fields fall back to the template defaults, and that a food can be found by
//! its string handle - the mechanism a consumed-food record relies on.

use overseer::file_ops::OverseerFileHandler;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

/// `SourceRegistry` is global and each parse resets it; cargo runs a binary's tests on
/// parallel threads, so they have to be serialised to stay deterministic.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/weight_tracker/foods.os");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {:?}: {}", path, err))
}

fn document() -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(&source()).expect("foods.os should parse");
    resolver::resolve_document(&mut nodes);
    nodes
}

fn node<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> &'a OverseerNode {
    let mut cur = nodes
        .iter()
        .find(|n| n.name == path[0])
        .unwrap_or_else(|| panic!("no root node {:?}", path[0]));
    for seg in &path[1..] {
        cur = cur
            .children
            .iter()
            .find(|c| &c.name == seg)
            .unwrap_or_else(|| panic!("missing {:?} while resolving {:?}", seg, path));
    }
    cur
}

/// Values behind a fallback or formula are produced when read, not stored on the node.
fn number(nodes: &[OverseerNode], path: &[&str]) -> f64 {
    let owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    let n = node(nodes, path);
    match FormulaEvaluator::get_effective_value_for_node(n, &owned, nodes) {
        Ok(OverseerValue::Float(f)) => f,
        Ok(OverseerValue::Integer(i)) => i as f64,
        other => panic!("{:?} did not evaluate to a number: {:?}", path.join("/"), other),
    }
}

fn entry_named(nodes: &[OverseerNode], handle: &str) -> String {
    let catalog = node(nodes, &["food_catalog", "Catalog"]);
    for child in &catalog.children {
        let path = vec![
            "food_catalog".to_string(),
            "Catalog".to_string(),
            child.name.clone(),
            "handle".to_string(),
        ];
        if let Some(h) = child.children.iter().find(|c| c.name == "handle") {
            if let Ok(OverseerValue::String(s)) =
                FormulaEvaluator::get_effective_value_for_node(h, &path, nodes)
            {
                if s == handle {
                    return child.name.clone();
                }
            }
        }
    }
    panic!("no catalog entry with handle {:?}", handle);
}

fn close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{}: expected {}, got {}",
        what,
        expected,
        actual
    );
}

#[test]
fn catalog_parses_and_round_trips() {
    serialised(|| {
        let original = source();
        let nodes = document();
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert_eq!(out, original, "foods.os did not round-trip byte-for-byte");
    });
}

#[test]
fn per_portion_is_derived_from_per_100g_and_portion_weight() {
    serialised(|| {
        let nodes = document();
        let apple = entry_named(&nodes, "apple");
        let base = ["food_catalog", "Catalog", apple.as_str()];

        // Apple: 52 kcal/100 g at a 180 g portion.
        let p = [base.as_slice(), &["per_portion", "calories"]].concat();
        close(number(&nodes, &p), 93.6, "apple portion calories");

        let p = [base.as_slice(), &["per_portion", "sugar"]].concat();
        close(number(&nodes, &p), 18.72, "apple portion sugar");

        // Pizza slice: 340 kcal/100 g at 125 g.
        let pizza = entry_named(&nodes, "pizza_slice");
        let p = [
            ["food_catalog", "Catalog", pizza.as_str()].as_slice(),
            &["per_portion", "calories"],
        ]
        .concat();
        close(number(&nodes, &p), 425.0, "pizza portion calories");
    });
}

#[test]
fn omitted_fields_fall_back_to_template_defaults() {
    serialised(|| {
        let nodes = document();
        // Every catalogued food omits trans_fat, so it must come from the template.
        let apple = entry_named(&nodes, "apple");
        let p = [
            ["food_catalog", "Catalog", apple.as_str()].as_slice(),
            &["per_100g", "trans_fat"],
        ]
        .concat();
        close(number(&nodes, &p), 0.0, "apple trans fat default");

        // The coffee latte omits fibre entirely; the default is 0 there because it is stated.
        let coffee = entry_named(&nodes, "coffee_latte");
        let p = [
            ["food_catalog", "Catalog", coffee.as_str()].as_slice(),
            &["per_100g", "trans_fat"],
        ]
        .concat();
        close(number(&nodes, &p), 0.0, "coffee trans fat default");
    });
}

#[test]
fn foods_are_reachable_by_string_handle() {
    serialised(|| {
        let nodes = document();
        let catalog = node(&nodes, &["food_catalog", "Catalog"]);

        let handles: Vec<String> = catalog
            .children
            .iter()
            .filter_map(|c| c.children.iter().find(|f| f.name == "handle"))
            .filter_map(|h| match h.parameters.get("value") {
                Some(OverseerValue::String(s)) => Some(s.clone()),
                _ => None,
            })
            .collect();

        assert!(
            handles.contains(&"pizza_slice".to_string()),
            "expected a pizza_slice handle, found {:?}",
            handles
        );

        let mut sorted = handles.clone();
        sorted.sort();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            before,
            "handles must be unique, found duplicates in {:?}",
            handles
        );
    });
}

/// The "Add to catalog" button must actually append a new entry built from the draft.
#[test]
fn add_button_appends_the_draft_to_the_catalog() {
    serialised(|| {
        overseer::source_registry::SourceRegistry::reset();
        let (_rest, mut nodes) = parser::parse_document(&source()).expect("parse");
        resolver::resolve_document(&mut nodes);

        let before = node(&nodes, &["food_catalog", "Catalog"]).children.len();

        let path = vec![
            "food_catalog".to_string(),
            "NewFood".to_string(),
            "add".to_string(),
        ];
        overseer::actions::ActionExecutor::execute_event(&mut nodes, &path, "click")
            .expect("add button click should execute");

        let catalog = node(&nodes, &["food_catalog", "Catalog"]);
        assert_eq!(
            catalog.children.len(),
            before + 1,
            "clicking Add should append exactly one entry"
        );

        // The appended entry must carry the draft's values, not just template defaults.
        let added = catalog.children.last().expect("appended entry");
        let handle = added
            .children
            .iter()
            .find(|c| c.name == "handle")
            .and_then(|h| h.parameters.get("value").cloned());
        assert_eq!(
            handle,
            Some(OverseerValue::String("new_food".to_string())),
            "appended entry should carry the draft handle, got {:?}",
            handle
        );
    });
}
