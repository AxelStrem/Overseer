//! Opening a document again says the same thing as opening it fresh.
//!
//! Most of what these documents compute is a function of their text alone, so opening an unchanged
//! one need not work it all out again. What stops that being simply true is the clock: a task's
//! priority climbs by the day and its deadline passes, so some values really do differ between one
//! opening and the next.
//!
//! The graph knows which. The clock is recorded as a read like any field, so everything descending
//! from it can be worked out again and everything else handed back as it was. This is the test
//! that the two give the same document - with the clock pinned, because a value that reads the
//! hour is *meant* to differ when the hour does, and comparing across a moving clock would be
//! asking the wrong question.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::OverseerNode;

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

fn agrees(source: &str, what: &str) {
    // Opened once with the graph recorded, then opened again - which is where the shortcut runs.
    let reopened = pinned(|| {
        app_api::forget_dependencies();
        app_api::forget_baseline();
        app_api::load_document_with_dependencies(source.to_string()).expect("first");
        app_api::load_document(source.to_string()).expect("again")
    });

    // And the same document opened with nothing remembered at all.
    let fresh = pinned(|| {
        app_api::forget_dependencies();
        app_api::forget_baseline();
        app_api::load_document(source.to_string()).expect("fresh")
    });

    let reopened_values = computed(&reopened);
    let fresh_values = computed(&fresh);

    let differences: Vec<String> = fresh_values
        .iter()
        .filter(|(k, v)| reopened_values.get(*k) != Some(*v))
        .take(5)
        .map(|(k, v)| {
            format!(
                "\n    {k}\n      opened fresh: {v}\n      reopened:     {}",
                reopened_values
                    .get(k)
                    .cloned()
                    .unwrap_or_else(|| "(absent)".into())
            )
        })
        .collect();

    assert!(
        differences.is_empty(),
        "reopening {what} gave a different document than opening it fresh ({} of {} values):{}",
        fresh_values
            .iter()
            .filter(|(k, v)| reopened_values.get(*k) != Some(*v))
            .count(),
        fresh_values.len(),
        differences.join("")
    );
}

const READS_THE_CLOCK: &str = r#"
tab t (label="T") {
    timestamp opened (label="") = $(now())
    int day (label="") = $(day_of_month(today()))

    div (hidden=true) {
        div Thing (layout="horizontal") {
            string name (label="") = ""
            int size (label="") = 0
            int doubled (label="") = $(size * 2)
        }
    }

    list Things (entry=<Thing>, key="name") {
        - {
            - name = "one"
            - size = 3
        }
    }

    int total (label="") = $(/t/Things.map(|x| x/doubled).sum())
}
"#;

const NEVER_ASKS: &str = r#"
tab t (label="T") {
    float rate (label="") = 1.5

    div (hidden=true) {
        div Thing (layout="horizontal") {
            string name (label="") = ""
            float size (label="") = 0
            float scaled (label="") = $(size * /t/rate)
        }
    }

    list Things (entry=<Thing>, key="name") {
        - {
            - name = "one"
            - size = 4
        }
        - {
            - name = "two"
            - size = 6
        }
    }

    float total (label="") = $(/t/Things.map(|x| x/scaled).sum())
}
"#;

#[test]
fn a_document_that_never_asks_the_time_is_handed_back_as_it_was() {
    agrees(NEVER_ASKS, "a document with no clock in it");
}

#[test]
fn a_document_that_does_ask_still_agrees() {
    // The harder half: the clock-dependent values are worked out again and the rest are not, so
    // this fails if the graph does not know which are which.
    agrees(READS_THE_CLOCK, "a document that reads the clock");
}

#[test]
fn the_real_documents_agree() {
    // The ones it is for. `tasks` reads the clock on every open row; `tracker_v2` is the one where
    // reopening went from thirty-eight seconds to one.
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in [
        "../examples/tasks/tasks.os",
        "../examples/shopping/shopping.os",
        "../examples/blood_pressure/blood_pressure.os",
        "../examples/projects/project_template.os",
    ] {
        let Ok(source) = std::fs::read_to_string(here.join(name)) else {
            continue;
        };
        agrees(&source, name.rsplit('/').next().unwrap());
    }
}
