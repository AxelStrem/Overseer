//! The serializer's contract with the frontend.
//!
//! `NodeSourceSnapshot` is `#[serde(skip)]`, so a node that has been through the IPC boundary
//! never carries its snapshot - only `source_id`, which the serializer uses to recover the
//! snapshot from the `SourceRegistry`. Every save the app performs goes through nodes in that
//! shape, so this is the path that actually matters for user data, and the one that has to
//! stay byte-faithful.

use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::OverseerNode;

/// `SourceRegistry` is global and each round-trip resets it; cargo runs a binary's tests on
/// parallel threads, so they have to be serialised to stay deterministic.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn fixture() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/exercise_inline_edit.os");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {:?}: {}", path, err))
}

fn parse(source: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("document should parse");
    resolver::resolve_document(&mut nodes);
    nodes
}

/// What actually reaches the backend: serde drops the snapshot, keeps everything else.
fn across_ipc(nodes: &[OverseerNode]) -> Vec<OverseerNode> {
    let json = serde_json::to_string(nodes).expect("serialize to json");
    serde_json::from_str(&json).expect("deserialize from json")
}

#[test]
fn nodes_that_crossed_the_ipc_boundary_still_round_trip() {
    serialised(|| {
        let original = fixture();
        let nodes = parse(&original);
        let payload = across_ipc(&nodes);

        assert!(
            payload.iter().all(|n| n.source_snapshot.is_none()),
            "precondition: the snapshot must not survive serde, or this test proves nothing"
        );

        let out = OverseerFileHandler::serialize_nodes(&payload).expect("serialize nodes");
        assert_eq!(
            out, original,
            "a document saved through the IPC boundary drifted from its source"
        );
    });
}

/// Explicit type keywords are part of the authored text. Losing them rewrites
/// `int p1 = 1` into `- p1 = 1`, which is how the corruption showed up on disk.
#[test]
fn explicit_type_keywords_survive_the_ipc_boundary() {
    serialised(|| {
        let original = fixture();
        let typed_before = original.matches("int p1").count();
        assert!(typed_before > 0, "fixture should contain explicit int fields");

        let nodes = parse(&original);
        let out =
            OverseerFileHandler::serialize_nodes(&across_ipc(&nodes)).expect("serialize nodes");

        assert_eq!(
            out.matches("int p1").count(),
            typed_before,
            "explicit `int` keywords were rewritten; output now has {} dash-form entries",
            out.matches("- p1").count()
        );
    });
}

/// Action bodies live as child nodes of their button. An emptied `on click` block is how
/// `button done { }` appeared on disk after a save.
#[test]
fn nested_bodies_are_not_emptied_across_the_ipc_boundary() {
    serialised(|| {
        let source = "div root {\n\
             \x20   button done (label=\"Done\") {\n\
             \x20       on click {\n\
             \x20           set (mode=\"value\", path=\"/root/x\") = 1\n\
             \x20       }\n\
             \x20   }\n\
             }\n";
        let nodes = parse(source);
        let out =
            OverseerFileHandler::serialize_nodes(&across_ipc(&nodes)).expect("serialize nodes");

        assert!(
            out.contains("set (mode=\"value\", path=\"/root/x\")"),
            "action body was dropped on save:\n{}",
            out
        );
        assert_eq!(out, source, "action block round-trip drifted");
    });
}
