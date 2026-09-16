//! Whether the graph can be trusted to say what an edit affects.
//!
//! This is the gate before anything uses it. Recomputing only what an edit reaches is worth
//! seconds per keystroke, and is worth nothing at all if the graph misses something: the missed
//! value keeps its old answer, looks like a real one, and nothing reports it. Slow and right beats
//! fast and quietly wrong, so the graph has to earn the job.
//!
//! The check is mechanical and does not rely on reading any formula. Change one field, resolve the
//! whole document the slow way, and see which computed values moved. Every one of them must appear
//! in the cascade the graph predicted from that same field. The cascade may be larger - it is
//! deliberately coarse, and recomputing too much only costs time - but it may never be smaller.
//!
//! Run against the real documents, because the fixtures cannot be relied on to contain the shapes
//! that break it. That was the lesson from the last graph, which looked right on small examples
//! and reported nothing for an aggregate.

use overseer::dependencies::{self, Graph};
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

/// Every computed value in the document, by path.
fn computed(nodes: &[OverseerNode]) -> std::collections::HashMap<String, String> {
    fn walk(
        nodes: &[OverseerNode],
        trail: &mut Vec<String>,
        out: &mut std::collections::HashMap<String, String>,
    ) {
        for (idx, n) in nodes.iter().enumerate() {
            // The same spelling the resolver uses: duplicate sibling names carry an ordinal, so
            // the second unnamed div is `div#1`. Without it the same node has two names and a
            // prediction about one looks like a miss about the other.
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
    let mut out = std::collections::HashMap::new();
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

/// Set the raw value at a path, by walking names.
fn put(nodes: &mut [OverseerNode], path: &[String], value: OverseerValue) -> bool {
    let Some((first, rest)) = path.split_first() else {
        return false;
    };
    // Every child of that name, not just the first: unnamed divs all answer to "div", and the
    // path walks through whichever one actually holds the rest.
    for node in nodes.iter_mut() {
        if &node.name != first {
            continue;
        }
        if rest.is_empty() {
            node.parameters.insert("value".to_string(), value);
            return true;
        }
        if put(&mut node.children, rest, value.clone()) {
            return true;
        }
    }
    false
}

/// A field somewhere in the document that holds a plain number, with its path.
fn some_numbers(nodes: &[OverseerNode], want: usize) -> Vec<(Vec<String>, f64)> {
    fn walk(
        nodes: &[OverseerNode],
        trail: &mut Vec<String>,
        out: &mut Vec<(Vec<String>, f64)>,
        want: usize,
    ) {
        for n in nodes {
            if out.len() >= want {
                return;
            }
            trail.push(n.name.clone());
            match n.parameters.get("value") {
                Some(OverseerValue::Integer(i)) => out.push((trail.clone(), *i as f64)),
                Some(OverseerValue::Float(f)) => out.push((trail.clone(), *f)),
                _ => {}
            }
            walk(&n.children, trail, out, want);
            trail.pop();
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut Vec::new(), &mut out, want);
    out
}

/// Resolve once with the clock pinned, recording what everything was worked out from.
fn resolved_with_graph(source: &str) -> (Vec<OverseerNode>, Graph) {
    let when = chrono::DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z")
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse");
    dependencies::start_recording();
    resolver::resolve_document(&mut nodes);
    let graph = dependencies::take_recording();
    FormulaEvaluator::set_time_override(None);
    (nodes, graph)
}

/// What an edit to this field actually moves, and what the graph said it would.
///
/// Returns the values that changed and were *not* predicted. Empty is the answer wanted.
fn missed_by_the_graph(source: &str, field: &[String], to: f64) -> Vec<String> {
    let (before_nodes, graph) = resolved_with_graph(source);
    let before = computed(&before_nodes);

    let when = chrono::DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z")
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(when));
    let (_rest, mut edited) = parser::parse_document(source).expect("parse");
    // Resolve first so the tree has the same shape - templates materialised, mounts in place -
    // and then make the edit and resolve again, which is what an edit actually does.
    resolver::resolve_document(&mut edited);
    assert!(
        put(&mut edited, field, OverseerValue::Float(to)),
        "no field at {field:?} to edit"
    );
    resolver::resolve_document(&mut edited);
    FormulaEvaluator::set_time_override(None);
    let after = computed(&edited);

    let edited_path = field.join("/");

    // Compared by node rather than by shadow key, because a node is the unit the cascade deals
    // in: naming it means re-evaluating it, and that recomputes every shadow it has. A node
    // carries several - the value, the template's copy of the value, one per computed parameter -
    // and they all move together. Asking the graph to name each of them separately would be
    // asking it a finer question than the one it is for.
    // The library's rule, not a second one written here. Splitting on the first `#` cuts inside
    // `div#2` and names the wrong node - and doing it the same wrong way on both sides of this
    // comparison is how a missed dependency came to look like a covered one.
    let node_of = |key: &str| Graph::node_of(key).to_string();

    let predicted: std::collections::HashSet<String> = graph
        .cascade(&[edited_path.clone(), format!("{edited_path}#_computed_value")])
        .into_iter()
        .map(|k| node_of(&k))
        .collect();

    let mut missed = std::collections::HashSet::new();
    for (path, new_value) in &after {
        let moved = before.get(path).map(|old| old != new_value).unwrap_or(true);
        if !moved {
            continue;
        }
        // The edited field's own shadows are the caller's business, not the graph's.
        if path.starts_with(&edited_path) {
            continue;
        }
        let node = node_of(path);
        if !predicted.contains(&node) {
            missed.insert(node);
        }
    }
    let mut missed: Vec<String> = missed.into_iter().collect();
    missed.sort();
    missed
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

#[test]
fn an_edit_to_a_line_reaches_everything_downstream_of_it() {
    // Three hops: the line's own tax, the total that adds it up, and the two values reading that.
    let missed = missed_by_the_graph(
        SHOP,
        &["shop".into(), "Lines".into(), "Line__1".into(), "price".into()],
        250.0,
    );
    assert!(
        missed.is_empty(),
        "the graph did not predict these, so an edit would leave them stale: {missed:?}"
    );
}

#[test]
fn an_edit_to_something_everything_reads_reaches_everything() {
    let missed = missed_by_the_graph(SHOP, &["shop".into(), "vat".into()], 0.5);
    assert!(
        missed.is_empty(),
        "the graph did not predict these: {missed:?}"
    );
}

#[test]
fn the_real_documents_hold_up() {
    // The ones the fixtures cannot stand in for. Several fields in each, so a single lucky shape
    // does not carry the result.
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    let mut complaints = Vec::new();

    for name in [
        "../examples/weight_tracker/tracker_v2.os",
        "../examples/tasks/tasks.os",
        "../examples/projects/project_template.os",
        "../examples/blood_pressure/blood_pressure.os",
    ] {
        let Ok(source) = std::fs::read_to_string(here.join(name)) else {
            continue;
        };
        let (nodes, _) = resolved_with_graph(&source);
        // Numbers, because they can be changed to something else without changing the shape of
        // the document - a different string might make a filter match differently, which is a
        // different question from the one being asked here.
        for (path, was) in some_numbers(&nodes, 200).into_iter().rev().take(6) {
            checked += 1;
            let missed = missed_by_the_graph(&source, &path, was + 7.0);
            if !missed.is_empty() {
                complaints.push(format!(
                    "{}: editing {} left {} value(s) unpredicted, e.g. {}",
                    name.rsplit('/').next().unwrap(),
                    path.join("/"),
                    missed.len(),
                    missed.first().cloned().unwrap_or_default()
                ));
            }
        }
    }

    assert!(checked > 0, "no fields were checked");
    assert!(
        complaints.is_empty(),
        "{} of {} edits were not fully predicted:\n{}",
        complaints.len(),
        checked,
        complaints.join("\n")
    );
}
