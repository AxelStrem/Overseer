//! What a project tracker says about itself, given no typed percentages at all.
//!
//! A leaf here is done or it is not, and one still on the open list is not - so every percentage
//! in the document is worked out from the tree beneath it. That makes the figures a thing worth
//! asserting: nothing on screen is a number someone entered, so a wrong rollup looks exactly like
//! a right one. The sample rows are chosen so the arithmetic is exact and can be checked by hand:
//!
//!   parser  = one open 2-pointer and a finished 6-pointer  -> 600 / 8  = 75%
//!   editor  = parser(8) + rows(4) + undo(5) + escape(5)    -> 1100 / 22 = 50%
//!   docs    = nothing open, 3 points finished              -> 300 / 3  = 100%
//!
//! And the second thing, which is the whole point of the design: `docs` reads 100% and is still
//! open. Its `finish` is offered and until it is pressed its own points are unpaid, so more work
//! can be put under a goal whose current children are all done.

use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

const TEMPLATE: &str = include_str!("../../examples/projects/project_template.os");

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(hit) = find(&node.children, name) {
            return Some(hit);
        }
    }
    None
}

fn child<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
    node.children.iter().find(|c| c.name == name)
}

/// What the row shows, computed or declared. The resolver leaves a formula's answer under a
/// shadow key and leaves the formula itself in place, so the shadow is what to read.
fn param(node: &OverseerNode, key: &str) -> Option<OverseerValue> {
    node.parameters
        .get(&format!("_computed_{}", key))
        .or_else(|| node.parameters.get(key))
        .cloned()
}

fn number(entry: &OverseerNode, field: &str) -> f64 {
    let node = child(entry, field).unwrap_or_else(|| panic!("no `{}` on {}", field, entry.name));
    match param(node, "value") {
        Some(OverseerValue::Integer(i)) => i as f64,
        Some(OverseerValue::Float(f)) => f,
        Some(OverseerValue::String(s)) => s
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("`{}` on {} reads {:?}", field, entry.name, s)),
        other => panic!("`{}` on {} is {:?}", field, entry.name, other),
    }
}

fn text(entry: &OverseerNode, field: &str) -> String {
    let Some(node) = child(entry, field) else {
        return String::new();
    };
    match param(node, "value") {
        Some(OverseerValue::String(s)) => s,
        Some(OverseerValue::Timestamp(s)) | Some(OverseerValue::Date(s)) => s,
        Some(OverseerValue::Integer(i)) => i.to_string(),
        Some(OverseerValue::Float(f)) => f.to_string(),
        _ => String::new(),
    }
}

/// Whether the renderer would draw this at all.
fn hidden(entry: &OverseerNode, field: &str) -> bool {
    let node = child(entry, field).unwrap_or_else(|| panic!("no `{}` on {}", field, entry.name));
    match param(node, "hidden") {
        Some(OverseerValue::Boolean(b)) => b,
        Some(OverseerValue::String(s)) => s.eq_ignore_ascii_case("true"),
        Some(OverseerValue::Integer(i)) => i != 0,
        _ => false,
    }
}

fn resolved(source: &str) -> Vec<OverseerNode> {
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse the project template");
    resolver::resolve_document(&mut nodes);
    nodes
}

/// The open items, by handle.
fn items(nodes: &[OverseerNode]) -> Vec<(String, &OverseerNode)> {
    find(nodes, "Items")
        .expect("no Items list")
        .children
        .iter()
        .map(|entry| (text(entry, "handle"), entry))
        .collect()
}

fn by_handle<'a>(nodes: &'a [OverseerNode], handle: &str) -> &'a OverseerNode {
    items(nodes)
        .into_iter()
        .find(|(h, _)| h == handle)
        .unwrap_or_else(|| panic!("no open task with handle `{}`", handle))
        .1
}

#[test]
fn the_percentages_are_worked_out_from_the_tree() {
    let nodes = resolved(TEMPLATE);

    for (handle, expected) in [("parser", 75.0), ("editor", 50.0), ("docs", 100.0)] {
        let entry = by_handle(&nodes, handle);
        assert_eq!(
            number(entry, "done"),
            expected,
            "`{}` reads {}% and should read {}% - weight {}, {} open children of {}",
            handle,
            number(entry, "done"),
            expected,
            number(entry, "weight"),
            number(entry, "open_kids"),
            number(entry, "kids"),
        );
    }
}

#[test]
fn an_open_leaf_reads_nothing() {
    // Not a rounding accident: a leaf on the open list has not been begun, because a leaf that
    // had been begun would have been split and one that was finished would be in History. This
    // is what makes the figure above it meaningful, so it is worth pinning.
    let nodes = resolved(TEMPLATE);

    for (handle, entry) in items(&nodes) {
        if number(entry, "kids") > 0.0 {
            continue;
        }
        assert_eq!(
            number(entry, "done"),
            0.0,
            "leaf `{}` reads {}%, which nothing should be able to type",
            handle,
            number(entry, "done"),
        );
        assert!(
            hidden(entry, "done"),
            "leaf `{}` shows a percentage; it has nothing to work one out from",
            handle,
        );
    }
}

#[test]
fn finish_waits_for_the_children_and_not_for_the_percentage() {
    let nodes = resolved(TEMPLATE);

    // Outstanding work beneath them, so pressing finish would orphan it.
    for handle in ["editor", "parser"] {
        let entry = by_handle(&nodes, handle);
        assert!(
            hidden(entry, "finish"),
            "`{}` offers finish with {} children still open",
            handle,
            number(entry, "open_kids"),
        );
    }

    // The state the design exists for: everything under it is done, and it is still open.
    let docs = by_handle(&nodes, "docs");
    assert_eq!(number(docs, "open_kids"), 0.0);
    assert!(
        !hidden(docs, "finish"),
        "`docs` has nothing open beneath it and does not offer finish, so its points can never \
         be claimed",
    );

    // And a leaf always offers it, having nothing to wait for.
    for handle in ["brace", "rows", "undo", "favicon"] {
        let entry = by_handle(&nodes, handle);
        assert!(!hidden(entry, "finish"), "leaf `{}` cannot be finished", handle);
    }
}

#[test]
fn the_last_movement_is_read_rather_than_stamped() {
    // It used to be written by a button, which made it true only when the button was remembered.
    // For a task with finished children it is the most recent of those finishes; for a leaf, when
    // it was added, which answers the same question about something that has not moved at all.
    let nodes = resolved(TEMPLATE);

    for (handle, expected) in [
        ("docs", "2026-09-10T15:10:00+04:00"),
        ("parser", "2026-09-09T11:00:00+04:00"),
        ("editor", "2026-09-05T17:20:00+04:00"),
        ("favicon", "2026-09-04T16:00:00+04:00"),
    ] {
        let entry = by_handle(&nodes, handle);
        assert_eq!(
            text(entry, "moved_at"),
            expected,
            "`{}` last moved at {}",
            handle,
            text(entry, "moved_at"),
        );
    }

    // The failure worth naming: `max` of an empty list answers the string "null", which would
    // reach the screen as a date that is not one.
    for (handle, entry) in items(&nodes) {
        let shown = text(entry, "moved_at");
        assert!(
            !shown.is_empty() && shown != "null",
            "`{}` shows {:?} for when it last moved",
            handle,
            shown,
        );
    }
}

#[test]
fn a_tree_worth_nothing_reads_nought_rather_than_failing() {
    // Division by zero is a hard error in the evaluator, and `done` divides by the points beneath
    // it. Defaulting a new task to one point avoids it in practice; this is the other half, for a
    // subtree whose points have been zeroed by hand. The formula under test is the real one - the
    // source is edited rather than a small copy of it written here, which would drift.
    // Both halves, or this proves nothing: dropping the per-entry overrides alone leaves every
    // task falling back on the template's one point, and the division never reaches zero.
    //
    // Anchored on the declaration rather than on the text of its parameters, which have changed
    // twice - the guard below caught it the second time, when the label became a table heading.
    let zeroed: String = TEMPLATE
        .replace("- points = ", "- points_unused = ")
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("int points (") {
                line.replace("= 1", "= 0")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        zeroed.contains("= 0") && !zeroed.contains("\n            - points = "),
        "the template moved; this test is no longer zeroing anything",
    );
    let nodes = resolved(&zeroed);

    for (handle, entry) in items(&nodes) {
        let node = child(entry, "done").expect("no done field");
        let reading = param(node, "value");
        assert!(
            !matches!(&reading, Some(OverseerValue::String(s)) if s.contains("invalid")),
            "`{}` reads {:?} once nothing under it is worth any points",
            handle,
            reading,
        );
    }
}

#[test]
fn no_percentage_is_typed_anywhere() {
    // The rework's substance: there is no field for partial progress on a leaf and no button that
    // advances one. Work part-way through a task is recorded by splitting the task, which credits
    // the same points on the same day and says what was done as well as how much.
    for (number, line) in TEMPLATE.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        for gone in ["int own", "button advance", "- own ="] {
            assert!(
                !code.contains(gone),
                "line {} still has `{}`: {}",
                number + 1,
                gone,
                line.trim(),
            );
        }
    }
}
