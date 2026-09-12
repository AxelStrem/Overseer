//! Every real document, read and written back, byte for byte.
//!
//! The strongest check available on a parser or serializer change, because these are the files
//! the change can actually damage: the ones holding two years of someone's records. A unit test
//! on a fixture proves the new construct works; this proves the old ones still do.
//!
//! Skipped rather than failed when the documents are not on this machine, so the suite still
//! runs for anyone who has only the repository.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;

/// Every `.os` file under a directory, if the directory is there at all.
fn documents(at: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(at) else {
        return Vec::new();
    };
    let mut found: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("os"))
        .collect();
    found.sort();
    found
}

fn round_trips(path: &std::path::Path) -> Result<(), String> {
    let original = std::fs::read_to_string(path).map_err(|e| format!("could not read: {e}"))?;
    // Named, so that anything it mounts resolves against its own directory rather than the
    // working directory of the test.
    let directory = path.parent().map(|d| d.to_path_buf());
    let nodes = DocumentManager::with_document(directory, || app_api::load_document(original.clone()))
        .map_err(|e| format!("could not load: {e:?}"))?;
    let written = OverseerFileHandler::serialize_nodes(&nodes)
        .map_err(|e| format!("could not serialize: {e}"))?;
    if written == original {
        return Ok(());
    }
    // Say where, not just that. A diff of two 100 KB documents is unreadable; the first line
    // that differs is usually the whole story.
    let mut lines = original.lines().zip(written.lines()).enumerate();
    let differs = lines.find(|(_, (before, after))| before != after);
    Err(match differs {
        Some((n, (before, after))) => format!(
            "line {} changed\n      was: {}\n      now: {}",
            n + 1,
            before.trim_end(),
            after.trim_end()
        ),
        None => format!(
            "same lines, different length: {} bytes became {}",
            original.len(),
            written.len()
        ),
    })
}

#[test]
fn the_documents_in_use_survive_a_round_trip() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let places = [
        here.join("../../Secrebot-Docs/personal-stats/documents"),
        here.join("../../Secrebot/documents"),
    ];

    let mut checked = 0;
    let mut broken = Vec::new();
    for place in &places {
        for document in documents(place) {
            checked += 1;
            if let Err(why) = round_trips(&document) {
                broken.push(format!(
                    "  {}: {}",
                    document.file_name().unwrap_or_default().to_string_lossy(),
                    why
                ));
            }
        }
    }

    if checked == 0 {
        eprintln!("no real documents on this machine; nothing to check");
        return;
    }
    assert!(
        broken.is_empty(),
        "{} of {} documents did not survive:\n{}",
        broken.len(),
        checked,
        broken.join("\n")
    );
    eprintln!("{checked} documents in use round-tripped unchanged");
}

/// Examples that did not round-trip before this test existed, and still do not.
///
/// Old scratch demos, none of them in use: a chart written `chart x(type=pie)` with no space, a
/// quoted string used as a node name, a leading comment on the first line. Listed rather than
/// quietly skipped, so the number can only go down - and so that a change which breaks a
/// seventh has somewhere to show up.
const ALREADY_BROKEN: &[&str] = &[
    "ex2.os",
    "example.os",
    "font_size_test.os",
    "grid_layout_showcase.os",
    "link_proxy_basic.os",
    "test_border_overflow.os",
];

#[test]
fn the_examples_survive_a_round_trip() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let examples = here.join("../examples");

    let mut checked = 0;
    let mut broken = Vec::new();
    // The examples sit in a directory per document, plus a few loose at the top.
    let mut places = vec![examples.clone()];
    if let Ok(entries) = std::fs::read_dir(&examples) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                places.push(entry.path());
            }
        }
    }
    places.sort();

    let mut mended = Vec::new();
    for place in &places {
        for document in documents(place) {
            let name = document
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let known = ALREADY_BROKEN.contains(&name.as_str());
            checked += 1;
            match (round_trips(&document), known) {
                (Err(why), false) => broken.push(format!("  {name}: {why}")),
                (Ok(()), true) => mended.push(name),
                _ => {}
            }
        }
    }

    assert!(checked > 0, "no examples found, so this checked nothing");
    assert!(
        broken.is_empty(),
        "{} of {} examples stopped surviving a round trip:\n{}",
        broken.len(),
        checked,
        broken.join("\n")
    );
    // Not a failure, but worth saying: something got fixed and the list should shrink.
    if !mended.is_empty() {
        eprintln!("these round-trip now and can leave ALREADY_BROKEN: {mended:?}");
    }
    eprintln!(
        "{} of {checked} examples round-tripped unchanged; {} known-broken",
        checked - ALREADY_BROKEN.len(),
        ALREADY_BROKEN.len()
    );
}
