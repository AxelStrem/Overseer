use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::OverseerNode;

fn load_fixture() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/weight_tracker/weight_tracker_new.os");
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

fn diff_snapshot(original: &str, regenerated: &str) -> String {
    use std::fmt::Write;
    let mut report = String::new();
    let mut a_lines = original.lines();
    let mut b_lines = regenerated.lines();
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
fn weight_tracker_round_trip_preserves_text() {
    overseer::source_registry::SourceRegistry::reset();

    let original = load_fixture();
    let (_rest, mut nodes) = parser::parse_document(&original).expect("document should parse");
    resolver::resolve_document(&mut nodes);

    fn debug_node(node: &OverseerNode, depth: usize) {
        let indent = "  ".repeat(depth);
        println!(
            "{}node: type={} name={} from_template={} source_id={} snapshot={} leading_blank_lines={} children={}",
            indent,
            node.node_type,
            node.name,
            matches!(
                node.parameters.get("_from_template"),
                Some(overseer::types::OverseerValue::Boolean(true))
            ),
            node.source_id.is_some(),
            node.source_snapshot.is_some(),
            node.leading_blank_lines,
            node.children.len()
        );
        let snapshot = node
            .source_snapshot
            .clone()
            .or_else(|| node.source_id.as_deref().and_then(overseer::source_registry::SourceRegistry::get));
        if let Some(snapshot) = snapshot {
            if snapshot.leading_trivia.contains("Selected panel") {
                println!(
                    "{}  leading_trivia contains Selected panel comment: {:?}",
                    indent, snapshot.leading_trivia
                );
            }
            if node.name == "WeightRecord__1" {
                println!(
                    "{}WeightRecord__1 leading trivia debug: len={} repr={:?}",
                    indent,
                    snapshot.leading_trivia.len(),
                    snapshot.leading_trivia
                );
            }
        }
        for child in &node.children {
            debug_node(child, depth + 1);
        }
    }
    for node in &nodes {
        debug_node(node, 0);
    }

    strip_snapshots(&mut nodes);

    let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize nodes");

    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("debug_regenerated_weight.os"),
        &regenerated,
    )
    .expect("write debug regenerated output");

    if regenerated != original {
        let diff = diff_snapshot(&original, &regenerated);
        println!("Original lines 80-90:");
        for (idx, line) in original.lines().enumerate().skip(79).take(10) {
            println!("{:>4}: {:?}", idx + 1, line);
        }
        println!("Regenerated lines 80-90:");
        for (idx, line) in regenerated.lines().enumerate().skip(79).take(10) {
            println!("{:>4}: {:?}", idx + 1, line);
        }
        panic!(
            "weight_tracker_new.os did not round-trip byte-for-byte\n{}\nline counts: original={} regenerated={}",
            diff,
            original.lines().count(),
            regenerated.lines().count()
        );
    }
}
