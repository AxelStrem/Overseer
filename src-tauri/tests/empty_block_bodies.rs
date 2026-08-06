//! Nodes whose block body is empty.
//!
//! `div x { }` and `mount M (...) { }` carry no children, so the serializer has nothing to
//! write between the braces - and used to write no braces either, silently turning the node
//! into a bodiless declaration. The declaration still parses, which is why this went
//! unnoticed: the document simply came back slightly different each time it was saved.
//!
//! This only shows when the snapshot fast path declines and the node is reformatted, which
//! is what happens once anything about the node has changed - a mount gaining loaded
//! children, for instance. The tests therefore drop the fingerprint to force that path.

use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::OverseerNode;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

/// Serialize with the fast path available.
fn round_trip(source: &str) -> String {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse");
    resolver::resolve_document(&mut nodes);
    OverseerFileHandler::serialize_nodes(&nodes).expect("serialize")
}

/// Serialize after forcing every node through the formatter, as happens once a node has been
/// touched. The snapshots stay available; only the "unchanged, replay verbatim" shortcut goes.
fn round_trip_reformatted(source: &str) -> String {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("parse");
    resolver::resolve_document(&mut nodes);
    fn drop_fingerprints(nodes: &mut [OverseerNode]) {
        for n in nodes.iter_mut() {
            n.source_fingerprint = None;
            drop_fingerprints(&mut n.children);
        }
    }
    drop_fingerprints(&mut nodes);
    OverseerFileHandler::serialize_nodes(&nodes).expect("serialize")
}

#[test]
fn an_empty_block_keeps_its_braces_when_reformatted() {
    serialised(|| {
        let source = "div root {\n    div empty { }\n    int a = 1\n}\n";
        assert_eq!(round_trip(&source), source, "precondition: it round-trips normally");
        assert_eq!(
            round_trip_reformatted(&source),
            source,
            "an empty block lost its braces when reformatted"
        );
    });
}

#[test]
fn a_mount_declaration_keeps_its_braces_when_reformatted() {
    serialised(|| {
        let source = "div root {\n    mount FOODS (source=\"foods.os/Catalog\", lazy=false) { }\n    int a = 1\n}\n";
        assert_eq!(round_trip(&source), source, "precondition: it round-trips normally");
        assert_eq!(
            round_trip_reformatted(&source),
            source,
            "a mount lost its braces when reformatted - this is what a save did to tracker_v2.os"
        );
    });
}

/// A mount written without a body must stay without one; the fix must not invent braces.
#[test]
fn a_mount_written_without_a_body_stays_that_way() {
    serialised(|| {
        let source = "div root {\n    mount FOODS (source=\"foods.os/Catalog\")\n    int a = 1\n}\n";
        assert_eq!(
            round_trip_reformatted(&source),
            source,
            "braces were invented for a node that never had them"
        );
    });
}

/// An empty block spanning lines keeps its shape too.
#[test]
fn a_multiline_empty_block_keeps_its_braces_when_reformatted() {
    serialised(|| {
        let source = "div root {\n    div empty {\n    }\n    int a = 1\n}\n";
        assert_eq!(
            round_trip_reformatted(&source),
            source,
            "a multi-line empty block lost its braces when reformatted"
        );
    });
}
