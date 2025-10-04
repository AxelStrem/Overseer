use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::OverseerNode;

fn load_fixture() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/exercise_tracker/exercise.os");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {:?}: {}", path, err))
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

#[test]
fn exercise_inline_edit_preserves_local_diff() {
    overseer::source_registry::SourceRegistry::reset();

    let original = load_fixture();
    let (_rest, mut nodes) = parser::parse_document(&original).expect("document should parse");
    resolver::resolve_document(&mut nodes);

    fn debug_names(nodes: &[OverseerNode], depth: usize) {
        for node in nodes {
            println!(
                "{}node type={} name={}",
                " ".repeat(depth * 2),
                node.node_type,
                node.name
            );
            if !node.children.is_empty() {
                debug_names(&node.children, depth + 1);
            }
        }
    }
    println!("tree before updates:");
    debug_names(&nodes, 0);

    // Locate the inline plates block under the second list entry and tweak p2.
    let exercises_list =
        find_node_mut(&mut nodes, &|node: &OverseerNode| node.name == "Exercises")
            .expect("Exercises list should exist");
    exercises_list.source_fingerprint = None;
    let exercise_entry = exercises_list
        .children
        .iter_mut()
        .find(|child| child.name == "Exercise__2")
        .expect("second exercise entry should exist");
    exercise_entry.source_fingerprint = None;
    println!(
        "entry snapshot? {} id {:?}",
        exercise_entry.source_snapshot.is_some(),
        exercise_entry.source_id
    );
    println!(
        "entry type={} name={}",
        exercise_entry.node_type,
        exercise_entry.name
    );
    let plates = exercise_entry
        .children
        .iter_mut()
        .find(|child| child.name == "plates")
        .expect("plates block present in exercise entry");
    println!(
        "plates snapshot present? {} source_id {:?}",
        plates.source_snapshot.is_some(),
        plates.source_id
    );
    println!(
        "plates type={} name={}",
        plates.node_type,
        plates.name
    );
    println!(
        "plates params: {:?}",
        plates
            .parameters
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    );
    plates.source_fingerprint = None;

    let mut updated = false;
    for child in plates.children.iter_mut() {
        println!("before serialize child {} value {:?}", child.name, child.parameters.get("value"));
        if child.name == "p2" {
            child.parameters
                .insert("value".to_string(), overseer::types::OverseerValue::Integer(42));
            child.source_fingerprint = None;
            child.parameters.insert(
                "_override_present".to_string(),
                overseer::types::OverseerValue::Boolean(true),
            );
            child.parameters.insert(
                "_explicit_child_override".to_string(),
                overseer::types::OverseerValue::Boolean(true),
            );
            child.parameters.remove("_template_value");
            child.authored_dash = true;
            updated = true;
            break;
        }
    }
    for child in plates.children.iter() {
        println!(
            "after update child {} value {:?} snapshot {:?}",
            child.name,
            child.parameters.get("value"),
            child
                .source_snapshot
                .as_ref()
                .map(|snap| snap.full_text.chars().take(40).collect::<String>())
        );
    }
    assert!(updated, "expected to update plates/p2 override");

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
    std::fs::write("debug_actual.os", &regenerated).unwrap();

    let expected = original.replacen(
        "                div plates {                    int p1 = 1\n                    int p2 = 1",
        "                div plates {                    int p1 = 1\n                    int p2 = 42",
        1,
    );
    std::fs::write("debug_expected.os", &expected).unwrap();
    if regenerated != expected {
        let diff = diff_snapshot(&expected, &regenerated);
        panic!(
            "inline edit introduced non-local diff\n{}\nexpected lines={} regenerated lines= {}",
            diff,
            expected.lines().count(),
            regenerated.lines().count()
        );
    }
}

#[test]
fn exercise_round_trip_preserves_text() {
    overseer::source_registry::SourceRegistry::reset();

    let original = load_fixture();
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
