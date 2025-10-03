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

fn diff_snapshot(a: &str, b: &str) -> String {
    use std::fmt::Write;
    let mut report = String::new();
    let mut a_lines = a.lines();
    let mut b_lines = b.lines();
    for idx in 1..=200 {
        match (a_lines.next(), b_lines.next()) {
            (Some(x), Some(y)) if x != y => {
                writeln!(&mut report, "line {} diff:\n  expected: {:?}\n  actual:   {:?}", idx, x, y).unwrap();
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
        report.push_str("no textual diff in first 50 lines; check lengths");
    }
    report
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
