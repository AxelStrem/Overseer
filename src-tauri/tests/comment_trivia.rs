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

/// From the third blank-line-separated leading comment block onwards, a blank line used to be
/// prepended to the top of the document - N blocks yielded N-2 of them - and since saving
/// rewrites the file they accumulated, one more on every save.
///
/// The blank lines between the blocks are part of the trivia's own text. The count of them was
/// emitted in front of it as well, which is that spacing said twice. Two blocks round-trip
/// either way, which is why `comment_block_split_by_blank_line_...` above never caught it, and
/// why `examples/weight_tracker/foods.os` had to be written as one contiguous block.
#[test]
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

#[test]
fn a_header_of_many_paragraphs_survives_too() {
    // It grew with the number of blocks, so the fix has to hold for more than three.
    let mut source = String::new();
    for n in 1..=6 {
        source.push_str(&format!("// paragraph {}\n// second line of it\n\n", n));
    }
    source.push_str("div root {\n    int a = 1\n}\n");
    assert_eq!(round_trip(&source), source);
}

#[test]
fn saving_repeatedly_does_not_accumulate_blank_lines() {
    // The part that made it expensive rather than untidy: every save added one more, so a
    // document written to by the bot grew a taller and taller gap above its first line.
    let source = "// one\n\n// two\n\n// three\n\n// four\ndiv root {\n    int a = 1\n}\n";
    let mut text = source.to_string();
    for save in 1..=5 {
        text = round_trip(&text);
        assert_eq!(
            text, source,
            "save number {} left {} blank line(s) at the top",
            save,
            text.chars().take_while(|c| *c == '\n').count()
        );
    }
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
