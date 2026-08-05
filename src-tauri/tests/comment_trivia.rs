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

/// Known bug: a bare `//` inside a leading comment block is dropped, and every
/// comment line above it is relocated to the end of the document. Since saving
/// rewrites the file, this loses and reorders the user's comments silently.
///
/// Observed output for the input below:
///   `// three\ndiv root {\n    int a = 1\n}\n// one\n`
///
/// Remove the `#[ignore]` once the trivia handling is fixed.
#[test]
#[ignore = "known bug: bare `//` line is dropped and preceding comments move to the document tail"]
fn bare_comment_marker_survives_round_trip() {
    let source = "// one\n//\n// three\ndiv root {\n    int a = 1\n}\n";
    assert_eq!(round_trip(source), source);
}
