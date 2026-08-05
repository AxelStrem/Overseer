//! Line-ending fidelity of the serializer.
//!
//! `.os` files are user data that the app rewrites on every save, so a document must come
//! back byte-identical in whichever newline style it was authored. These tests pin that
//! against a frozen fixture in both styles: the repo has `core.autocrlf=true` and no
//! `.gitattributes`, so a working copy flips between LF and CRLF depending on whether a
//! file was last written by git or by the app - which means a test reading a live example
//! only covers whichever style happens to be checked out right now.

use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::OverseerNode;

/// `SourceRegistry` is global, and every round-trip below resets it. Cargo runs the tests in
/// one binary on parallel threads, so without serialising them one test's reset lands in the
/// middle of another's parse and silently changes whether the snapshot fast path applies -
/// which made these assertions flip between runs. This is a distinct lock from the registry's
/// own `REGISTRY_TEST_MUTEX` (taken inside `serialize_nodes`), so it cannot deadlock against it.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn fixture_lf() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/exercise_inline_edit.os");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {:?}: {}", path, err));
    text.replace("\r\n", "\n")
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

/// Round-trip with snapshots intact, i.e. the fast path that replays source text verbatim.
fn round_trip(source: &str) -> String {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("document should parse");
    resolver::resolve_document(&mut nodes);
    OverseerFileHandler::serialize_nodes(&nodes).expect("serialize nodes")
}

/// Round-trip mirroring a runtime save, where snapshots have been dropped.
fn round_trip_without_snapshots(source: &str) -> String {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("document should parse");
    resolver::resolve_document(&mut nodes);
    strip_snapshots(&mut nodes);
    OverseerFileHandler::serialize_nodes(&nodes).expect("serialize nodes")
}

fn describe(text: &str) -> String {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    let doubled = text.matches("\r\r").count();
    format!("CRLF={} bare-LF={} doubled-CR={}", crlf, lf, doubled)
}

#[test]
fn lf_document_round_trips_byte_for_byte() {
    serialised(|| {
        let source = fixture_lf();
        assert_eq!(round_trip(&source), source, "LF round-trip drifted");
    });
}

#[test]
fn crlf_document_round_trips_byte_for_byte() {
    serialised(|| {
        let source = fixture_lf().replace('\n', "\r\n");
        let out = round_trip(&source);
        assert_eq!(
            out,
            source,
            "CRLF round-trip drifted: expected {}, produced {}",
            describe(&source),
            describe(&out)
        );
    });
}

#[test]
fn crlf_document_survives_a_save_without_snapshots() {
    serialised(|| {
        let source = fixture_lf().replace('\n', "\r\n");
        let out = round_trip_without_snapshots(&source);
        assert!(
            !out.contains("\r\r"),
            "serializer injected a doubled carriage return: {}",
            describe(&out)
        );
    });
}

/// Saving repeatedly must not accumulate anything. A newline bug that adds one CR per line
/// per save is invisible in a single round-trip assertion but corrupts a file over time.
#[test]
fn repeated_saves_are_idempotent() {
    serialised(|| {
        let variants: [(&str, fn(&str) -> String); 2] = [
            ("with snapshots", round_trip),
            ("runtime save", round_trip_without_snapshots),
        ];
        for (label, save) in variants {
            for source in [fixture_lf(), fixture_lf().replace('\n', "\r\n")] {
                let once = save(&source);
                let twice = save(&once);
                let thrice = save(&twice);
                assert!(
                    !once.contains("\r\r"),
                    "[{}] first save injected a doubled CR: {}",
                    label,
                    describe(&once)
                );
                assert_eq!(
                    once,
                    twice,
                    "[{}] second save drifted: {}",
                    label,
                    describe(&twice)
                );
                assert_eq!(
                    twice,
                    thrice,
                    "[{}] third save drifted: {}",
                    label,
                    describe(&thrice)
                );
                assert!(
                    !thrice.contains("\r\r"),
                    "[{}] carriage returns accumulated across saves: {}",
                    label,
                    describe(&thrice)
                );
            }
        }
    });
}
