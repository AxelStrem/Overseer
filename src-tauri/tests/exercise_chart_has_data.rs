//! Every plot in the exercise chart must resolve to points.
//!
//! This is the failure a chart does not report. When the exercise ids became string handles,
//! the plots kept filtering on the integers they used to be - `x/eid == 1` against `"bicep_curls"`
//! - and every series came back empty. The chart still drew: axes, legend, labels, colours, all
//! correct and nothing in it. It looked like a tracker with nothing recorded rather than a
//! broken formula, which is why it survived a migration and a round of testing.
//!
//! So the assertion is about data reaching the plots, not about the formulas being well formed.

use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

fn collect_plots<'a>(nodes: &'a [OverseerNode], out: &mut Vec<&'a OverseerNode>) {
    for node in nodes {
        if node.node_type == "plot" {
            out.push(node);
        }
        collect_plots(&node.children, out);
    }
}

/// What the renderer draws: the points the source formula resolved to.
fn series_len(plot: &OverseerNode) -> Option<usize> {
    let series = plot.parameters.get("_computed_series")?;
    let text = match series {
        OverseerValue::String(s) => s.clone(),
        other => format!("{:?}", other),
    };
    let trimmed = text.trim();
    if trimmed == "[]" {
        return Some(0);
    }
    // Counting entries rather than parsing: the shape of a point is the renderer's business,
    // and a test that knew it would break when that changed for reasons that are not this bug.
    Some(trimmed.matches("{").count().max(usize::from(!trimmed.is_empty())))
}

fn label(plot: &OverseerNode) -> String {
    match plot.parameters.get("label") {
        Some(OverseerValue::String(s)) => s.clone(),
        _ => plot.name.clone(),
    }
}

#[test]
fn every_plot_in_the_exercise_chart_has_points() {
    let content = include_str!("../../examples/exercise_tracker/exercise.os");
    let (_rest, mut nodes) = parser::parse_document(content).expect("parse exercise tracker");
    resolver::resolve_document(&mut nodes);

    let mut plots = Vec::new();
    collect_plots(&nodes, &mut plots);
    assert!(
        !plots.is_empty(),
        "no plots found at all - the chart may have been renamed or removed"
    );

    let empty: Vec<String> = plots
        .iter()
        .filter(|plot| series_len(plot).unwrap_or(0) == 0)
        .map(|plot| format!("{} ({})", plot.name, label(plot)))
        .collect();

    assert!(
        empty.is_empty(),
        "these plots resolved to nothing, so the chart draws empty: {}",
        empty.join(", ")
    );
}

#[test]
fn no_plot_still_filters_on_a_number() {
    // The specific mistake, named. Exercises are referred to by handle now, and a plot left
    // comparing against a number matches nothing - silently, because a filter that finds
    // nothing is indistinguishable from an exercise never done.
    let content = include_str!("../../examples/exercise_tracker/exercise.os");
    for (number, line) in content.lines().enumerate() {
        if line.contains("plot ") && line.contains("x/eid ==") {
            let comparand = line
                .split("x/eid ==")
                .nth(1)
                .and_then(|rest| rest.split(')').next())
                .unwrap_or("")
                .trim()
                .to_string();
            assert!(
                comparand.starts_with('"'),
                "line {} compares eid to {}, which is not a handle",
                number + 1,
                comparand
            );
        }
    }
}
