//! An edit answered from the dependency graph says the same thing as resolving everything.
//!
//! This is the claim the speed rests on. The edit path no longer works out every formula in the
//! document - it works out what the graph says the edit reaches - and that is only worth doing if
//! the answer is identical. A value left stale by a missed dependency looks exactly like a real
//! one, and nothing reports it, so the comparison has to be whole-document and exact.
//!
//! Different from `the_cascade_covers_the_change`, which asks whether the graph *predicts* the
//! right set. This asks whether the machinery that uses the prediction produces the right
//! document, which is the question a person actually has.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::{OverseerNode, OverseerValue};

/// Every computed value, spelled the way the resolver spells paths.
fn computed(nodes: &[OverseerNode]) -> std::collections::BTreeMap<String, String> {
    fn walk(
        nodes: &[OverseerNode],
        trail: &mut Vec<String>,
        out: &mut std::collections::BTreeMap<String, String>,
    ) {
        for (idx, n) in nodes.iter().enumerate() {
            let repeats = nodes.iter().take(idx).filter(|c| c.name == n.name).count();
            trail.push(if repeats > 0 {
                format!("{}#{}", n.name, repeats)
            } else {
                n.name.clone()
            });
            for (k, v) in &n.parameters {
                if k.starts_with("_computed_") {
                    out.insert(format!("{}#{}", trail.join("/"), k), format!("{v:?}"));
                }
            }
            walk(&n.children, trail, out);
            trail.pop();
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

fn pinned<T>(work: impl FnOnce() -> T) -> T {
    let when = chrono::DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z")
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));
    let out = work();
    FormulaEvaluator::set_time_override(None);
    out
}

/// The document after an edit, worked out the quick way and the slow way.
fn both_ways(source: &str, field: &str, to: OverseerValue) -> (Vec<OverseerNode>, Vec<OverseerNode>) {
    // The quick way: open the document, which builds the graph, then send the edit as the
    // frontend does - the text it holds, the field that changed, and its new value.
    let quick = pinned(|| {
        // Recorded explicitly: building the graph is not what opening a document does by
        // default, and a test that did not ask for one would compare the full resolve with
        // itself and pass without touching the thing it is about.
        app_api::load_document_with_dependencies(source.to_string()).expect("load");
        let mut values = std::collections::HashMap::new();
        values.insert(field.to_string(), to.clone());
        app_api::resolve_selective(
            source.to_string(),
            vec![field.to_string()],
            Some(values),
        )
        .expect("selective")
    });

    // The slow way: the same edit, with everything worked out afresh.
    let slow = pinned(|| {
        // No baseline and no graph, so this resolves everything - the answer to compare against.
        app_api::forget_baseline();
        app_api::forget_dependencies();
        let mut values = std::collections::HashMap::new();
        values.insert(field.to_string(), to);
        app_api::resolve_selective(source.to_string(), vec![field.to_string()], Some(values))
            .expect("selective")
    });

    (quick, slow)
}

const SHOP: &str = r#"
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

    float total (label="Total") = $(/shop/Lines.map(|x| x/taxed).sum())
    float doubled (label="") = $(/shop/total * 2)
    string verdict (label="") = $(/shop/total > 100 ? "dear" : "cheap")
}
"#;

fn agree(source: &str, field: &str, to: OverseerValue) {
    let (quick, slow) = both_ways(source, field, to);
    let quick_values = computed(&quick);
    let slow_values = computed(&slow);

    let differences: Vec<String> = slow_values
        .iter()
        .filter(|(k, v)| quick_values.get(*k) != Some(*v))
        .take(5)
        .map(|(k, v)| {
            format!(
                "\n    {k}\n      resolving everything: {v}\n      from the graph:       {}",
                quick_values.get(k).cloned().unwrap_or_else(|| "(absent)".into())
            )
        })
        .collect();

    assert!(
        differences.is_empty(),
        "editing {field} gave a different document than resolving everything would:{}",
        differences.join("")
    );
}

#[test]
fn a_leaf_edit_agrees() {
    agree(SHOP, "shop/Lines/Line__1/price", OverseerValue::Float(250.0));
}

#[test]
fn an_edit_everything_reads_agrees() {
    agree(SHOP, "shop/vat", OverseerValue::Float(0.5));
}

#[test]
fn an_edit_that_flips_a_condition_agrees() {
    // `verdict` reads a threshold, so this edit changes a string as well as the numbers.
    agree(SHOP, "shop/Lines/Line__2/price", OverseerValue::Float(1.0));
}

#[test]
fn the_real_documents_agree() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for (doc, field, to) in [
        (
            "../examples/projects/project_template.os",
            "project/Items/Item__1/points",
            OverseerValue::Float(9.0),
        ),
        (
            "../examples/tasks/tasks.os",
            "tasks/Rules/Rule__1/div/base_priority",
            OverseerValue::Float(42.0),
        ),
        (
            "../examples/blood_pressure/blood_pressure.os",
            "blood_pressure/History/Reading__1/measurements/Measurement__1/systolic",
            OverseerValue::Float(155.0),
        ),
    ] {
        let Ok(source) = std::fs::read_to_string(here.join(doc)) else {
            continue;
        };
        agree(&source, field, to);
    }
}
