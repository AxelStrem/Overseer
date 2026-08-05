//! Serializer handling of standalone comment trivia.

use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;

fn round_trip(source: &str) -> String {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("document should parse");
    resolver::resolve_document(&mut nodes);
    OverseerFileHandler::serialize_nodes(&nodes).expect("serialize nodes")
}

#[test]
fn leading_comment_block_survives_round_trip() {
    let source = "// one\n// two\n// three\ndiv root {\n    int a = 1\n}\n";
    assert_eq!(round_trip(source), source);
}

#[test]
fn comment_block_split_by_blank_line_survives_round_trip() {
    let source = "// one\n\n// three\ndiv root {\n    int a = 1\n}\n";
    assert_eq!(round_trip(source), source);
}

/// Known bug: from the *third* blank-line-separated leading comment block onwards, a blank
/// line is prepended to the top of the document - N blocks yield N-2 leading blank lines,
/// and since saving rewrites the file they accumulate on every save.
///
/// Two blocks round-trip cleanly, which is why `comment_block_split_by_blank_line_...`
/// above does not catch it. A file header of several paragraphs is the natural shape that
/// trips it; `examples/weight_tracker/foods.os` had to be written as one contiguous block
/// to avoid it.
///
/// Remove the `#[ignore]` once the leading-trivia handling is fixed.
#[test]
#[ignore = "known bug: 3+ blank-separated leading comment blocks prepend blank lines on every save"]
fn several_leading_comment_blocks_survive_round_trip() {
    let source = "// one\n\n// two\n\n// three\ndiv root {\n    int a = 1\n}\n";
    let out = round_trip(source);
    assert_eq!(
        out,
        source,
        "{} blank line(s) were prepended",
        out.chars().take_while(|c| *c == '\n').count()
    );
}

/// A bare `//` used as a separator must survive a round trip.
///
/// It used to be dropped, with every comment line above it relocated to the end of the
/// document, because `skip_single_line_comment` required at least one character after the
/// slashes and so failed on an empty one.
#[test]
fn bare_comment_marker_survives_round_trip() {
    let source = "// one\n//\n// three\ndiv root {\n    int a = 1\n}\n";
    assert_eq!(round_trip(source), source);
}

/// The same bare `//` is far more damaging *inside* a block: rather than merely relocating
/// the comment, the parser reads the text above it as DSL and builds a node per word.
///
/// Those nodes are invisible in the source but real in the document. In the calorie tracker
/// they rendered as a run of empty blocks after every day, and one of them - named `intake`,
/// from a comment that mentioned that path - shadowed the day's actual intake list, so
/// `append (list="../intake")` silently targeted the wrong node and the Add buttons appeared
/// to do nothing.
///
/// Same defect, and the reason it mattered: once the comment parser rejected the bare `//`,
/// nom backtracked and the node parser consumed the comment text instead.
#[test]
fn bare_comment_marker_inside_a_block_is_not_parsed_as_nodes() {
    let source = "div root {\n    div inner {\n        int a = 1\n        // one\n        //\n        // three\n        int b = 2\n    }\n}\n";
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("document should parse");
    resolver::resolve_document(&mut nodes);

    fn names(nodes: &[overseer::types::OverseerNode], out: &mut Vec<String>) {
        for n in nodes {
            out.push(n.name.clone());
            names(&n.children, out);
        }
    }
    let mut found = Vec::new();
    names(&nodes, &mut found);

    let expected = ["root", "inner", "a", "b"];
    let stray: Vec<&String> = found
        .iter()
        .filter(|n| !expected.contains(&n.as_str()))
        .collect();
    assert!(
        stray.is_empty(),
        "comment text was parsed into {} node(s): {:?}",
        stray.len(),
        stray
    );
}
