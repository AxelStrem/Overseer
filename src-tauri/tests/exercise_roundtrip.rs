use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::OverseerNode;

/// The live example document. It is real user data that changes whenever the app
/// is used, so only assertions that hold for *any* content may rely on it.
fn load_live_example() -> String {
    load_os_file(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/exercise_tracker/exercise.os"),
    )
}

/// A frozen fixture, safe to anchor assertions about specific nodes against.
fn load_frozen_fixture(name: &str) -> String {
    load_os_file(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
}

fn load_os_file(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {:?}: {}", path, err))
}

fn strip_snapshots(nodes: &mut [OverseerNode]) {
    for node in nodes.iter_mut() {
        node.source_snapshot = None;
        node.source_fingerprint = None;
        if !node.children.is_empty() {
            strip_snapshots(&mut node.children);
        }
    }
}

fn find_node_mut<'a, F>(nodes: &'a mut [OverseerNode], predicate: &F) -> Option<&'a mut OverseerNode>
where
    F: Fn(&OverseerNode) -> bool,
{
    for node in nodes.iter_mut() {
        if predicate(node) {
            return Some(node);
        }
        if let Some(found) = find_node_mut(&mut node.children, predicate) {
            return Some(found);
        }
    }
    None
}

fn diff_snapshot(a: &str, b: &str) -> String {
    use std::fmt::Write;
    let mut report = String::new();
    let mut a_lines = a.lines();
    let mut b_lines = b.lines();
    for idx in 1..=200 {
        match (a_lines.next(), b_lines.next()) {
            (Some(x), Some(y)) if x != y => {
                writeln!(
                    &mut report,
                    "line {} diff:\n  expected: {:?}\n  actual:   {:?}",
                    idx, x, y
                )
                .unwrap();
                break;
            }
            (None, Some(y)) => {
                writeln!(&mut report, "output has extra line {}: {:?}", idx, y).unwrap();
                break;
            }
            (Some(x), None) => {
                writeln!(&mut report, "output missing line {}: {:?}", idx, x).unwrap();
                break;
            }
            (None, None) => break,
            _ => continue,
        }
    }
    if report.is_empty() {
        report.push_str("no textual diff in first 200 lines; check lengths");
    }
    report
}

/// Every line that differs between two documents, as (line number, before, after).
/// Only meaningful for edits that preserve the line count; the callers assert that
/// separately so a length change is reported as its own failure.
fn changed_lines<'a>(before: &'a str, after: &'a str) -> Vec<(usize, &'a str, &'a str)> {
    before
        .lines()
        .zip(after.lines())
        .enumerate()
        .filter(|(_, (b, a))| b != a)
        .map(|(idx, (b, a))| (idx + 1, b, a))
        .collect()
}

fn render_changed_lines(diffs: &[(usize, &str, &str)]) -> String {
    use std::fmt::Write;
    let mut report = String::new();
    for (line, before, after) in diffs.iter().take(20) {
        writeln!(
            &mut report,
            "line {}:\n  before: {:?}\n  after:  {:?}",
            line, before, after
        )
        .unwrap();
    }
    if diffs.len() > 20 {
        writeln!(&mut report, "... and {} more", diffs.len() - 20).unwrap();
    }
    report
}

#[test]
fn exercise_inline_edit_preserves_local_diff() {
    overseer::source_registry::SourceRegistry::reset();

    let original = load_frozen_fixture("exercise_inline_edit.os");
    let (_rest, mut nodes) = parser::parse_document(&original).expect("document should parse");
    resolver::resolve_document(&mut nodes);

    // Locate the inline plates block under the second list entry and tweak p2.
    let exercises_list = find_node_mut(&mut nodes, &|node: &OverseerNode| node.name == "Exercises")
        .expect("Exercises list should exist");
    exercises_list.source_fingerprint = None;
    let exercise_entry = exercises_list
        .children
        .iter_mut()
        .find(|child| child.name == "Exercise__2")
        .expect("second exercise entry should exist");
    exercise_entry.source_fingerprint = None;
    let plates = exercise_entry
        .children
        .iter_mut()
        .find(|child| child.name == "plates")
        .expect("plates block present in exercise entry");
    plates.source_fingerprint = None;

    let target = plates
        .children
        .iter_mut()
        .find(|child| child.name == "p2")
        .expect("plates/p2 present in exercise entry");
    let previous_value = target.parameters.get("value").cloned();
    target
        .parameters
        .insert("value".to_string(), overseer::types::OverseerValue::Integer(42));
    target.source_fingerprint = None;
    target.parameters.insert(
        "_override_present".to_string(),
        overseer::types::OverseerValue::Boolean(true),
    );
    target.parameters.insert(
        "_explicit_child_override".to_string(),
        overseer::types::OverseerValue::Boolean(true),
    );
    target.parameters.remove("_template_value");
    target.authored_dash = true;
    assert_ne!(
        previous_value,
        Some(overseer::types::OverseerValue::Integer(42)),
        "fixture already holds the edited value, so the test would assert nothing"
    );

    let entry = plates
        .parameters
        .entry("_explicit_overrides".to_string())
        .or_insert(overseer::types::OverseerValue::String(String::new()));
    if let overseer::types::OverseerValue::String(list) = entry {
        if !list.split(',').any(|item| item == "p2") {
            if !list.is_empty() {
                list.push(',');
            }
            list.push_str("p2");
        }
    }

    let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize nodes");

    // The edit must show up as exactly one changed line, and it must be the one
    // we edited. Anything else means the serializer reflowed untouched regions.
    let diffs = changed_lines(&original, &regenerated);
    assert_eq!(
        diffs.len(),
        1,
        "inline edit introduced a non-local diff ({} lines changed):
{}",
        diffs.len(),
        render_changed_lines(&diffs)
    );
    let (_, before, after) = &diffs[0];
    assert_eq!(before.trim(), "int p2 = 1", "unexpected line was rewritten");
    assert_eq!(after.trim(), "int p2 = 42", "edit did not land on plates/p2");
    assert_eq!(
        original.lines().count(),
        regenerated.lines().count(),
        "inline edit changed the document line count"
    );
}

#[test]
fn exercise_round_trip_preserves_text() {
    overseer::source_registry::SourceRegistry::reset();

    // Deliberately the live example: whatever the user's real document grows into,
    // an untouched parse -> resolve -> serialize must return it byte-for-byte.
    let original = load_live_example();
    let (_rest, mut nodes) = parser::parse_document(&original).expect("document should parse");
    resolver::resolve_document(&mut nodes);

    strip_snapshots(&mut nodes);

    let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize nodes");

    if regenerated != original {
        let diff = diff_snapshot(&original, &regenerated);
        panic!(
            "exercise.os did not round-trip byte-for-byte\n{}\nline counts: original={} regenerated={}",
            diff,
            original.lines().count(),
            regenerated.lines().count()
        );
    }
}
