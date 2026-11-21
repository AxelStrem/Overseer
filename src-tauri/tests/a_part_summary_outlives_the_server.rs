//! The summaries a total over a list with parts reads are kept on disk, by the part's text, the
//! template and the chain, so a server that has just started summarises no part again - and never
//! hands back one made from another text or another template.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};

const DOCUMENT: &str = r#"tab log (label="Log", mutable=true) {
    div (hidden=true) {
        div Done (layout="horizontal") {
            string handle = ""
            string parent = ""
            string status = "done"
            timestamp done_at = "2026-11-02T10:00:00Z"
            int points = 1
        }
        div Item {
            string handle = ""
            int kids = $(/log/History.filter(|x| x/parent == ../handle && x/status != "cancelled").count())
        }
    }
    int points (mutable=false) = $(History.sum(|x| x/points))
    int won (mutable=false) = $(History.filter(|x| x/status != "cancelled").map(|x| x/points).sum())

    list Items (entry=<Item>, key="handle") {
        - {
            - handle = "p"
        }
    }

    list History (entry=<Done>, key="handle", window=1, parts=$(|x| week(x/done_at)),
                  sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) { }
}
"#;

const OLDEST: &str = r#"list History {
    - {
        - handle = "a"
        - parent = "p"
        - done_at = "2026-10-20T08:00:00Z"
        - points = 3
    }
    - {
        - handle = "d"
        - status = "cancelled"
        - done_at = "2026-10-21T08:00:00Z"
        - points = 8
    }
}
"#;

const OLDER: &str = r#"list History {
    - {
        - handle = "c"
        - parent = "p"
        - done_at = "2026-10-27T08:00:00Z"
        - points = 4
    }
}
"#;

const THIS_WEEK: &str = r#"list History {
    - {
        - handle = "b"
        - points = 5
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        overseer::document_disk::set_override(None);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A folder of documents, and a disk cache of its own beside it - or none, for `cache` false.
fn sandbox(tag: &str, cache: bool) -> Sandbox {
    let root = std::env::temp_dir().join(format!("overseer_sumdisk_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let documents = root.join("documents");
    std::fs::create_dir_all(documents.join("log.parts/History")).unwrap();
    std::fs::write(documents.join("log.os"), DOCUMENT).unwrap();
    std::fs::write(documents.join("log.parts/History/2026-W43.os"), OLDEST).unwrap();
    std::fs::write(documents.join("log.parts/History/2026-W44.os"), OLDER).unwrap();
    std::fs::write(documents.join("log.parts/History/2026-W45.os"), THIS_WEEK).unwrap();
    let setting = cache.then(|| (root.join("cache"), 64 * 1024 * 1024));
    overseer::document_disk::set_override(Some(setting));
    Sandbox(root)
}

/// The document opened as by a server that has just started: nothing of it held in memory.
fn open_fresh(sandbox: &Sandbox) -> Vec<OverseerNode> {
    overseer::document_disk::settled();
    overseer::partsums::forget_in_memory();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&sandbox.0.join("documents")).unwrap();
    documents.open("log.os").expect("open")
}

fn value(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let params = &overseer::addressing::find(nodes, address)?.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

fn totals(nodes: &[OverseerNode]) -> [Option<OverseerValue>; 3] {
    [value(nodes, "log/points"), value(nodes, "log/won"), value(nodes, "log/Items/[p]/kids")]
}

fn int(n: i64) -> Option<OverseerValue> {
    Some(OverseerValue::Integer(n))
}

fn summaries_on_disk(sandbox: &Sandbox) -> usize {
    overseer::document_disk::settled();
    std::fs::read_dir(sandbox.0.join("cache/sums"))
        .map(|listing| {
            listing.filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "json")).count()
        })
        .unwrap_or(0)
}

/// Write a part anew, and give it back the time it was written before, as a copy or a sync can.
fn rewrite_keeping_its_time(file: &Path, text: &str) {
    let was = std::fs::metadata(file).unwrap().modified().unwrap();
    std::fs::write(file, text).unwrap();
    std::fs::File::options().write(true).open(file).unwrap().set_modified(was).unwrap();
}

#[test]
fn a_server_that_has_just_started_reads_the_summaries_another_wrote() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("restart", true);
    let before = overseer::partsums::summaries_made();
    let nodes = open_fresh(&sandbox);
    assert!(overseer::partsums::summaries_made() > before, "the parts are summarised once");
    assert_eq!(totals(&nodes), [int(20), int(12), int(2)]);
    assert!(summaries_on_disk(&sandbox) > 0, "and the summaries written down");

    let made = overseer::partsums::summaries_made();
    let nodes = open_fresh(&sandbox);
    assert_eq!(overseer::partsums::summaries_made(), made, "no part is summarised again");
    assert_eq!(totals(&nodes), [int(20), int(12), int(2)]);
}

#[test]
fn a_part_edited_with_its_time_kept_is_summarised_again() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("edited", true);
    open_fresh(&sandbox);
    rewrite_keeping_its_time(
        &sandbox.0.join("documents/log.parts/History/2026-W44.os"),
        &OLDER.replace("points = 4", "points = 40"),
    );
    let nodes = open_fresh(&sandbox);
    assert_eq!(totals(&nodes), [int(56), int(48), int(2)]);
}

#[test]
fn a_changed_template_makes_its_summaries_anew() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("template", true);
    open_fresh(&sandbox);
    let document = sandbox.0.join("documents/log.os");
    std::fs::write(&document, DOCUMENT.replace(r#"string status = "done""#, r#"string status = "cancelled""#))
        .unwrap();
    let nodes = open_fresh(&sandbox);
    assert_eq!(totals(&nodes), [int(20), int(0), int(0)], "every entry without a status is now cancelled");
}

#[test]
fn with_the_disk_cache_off_no_summary_is_kept() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("off", false);
    open_fresh(&sandbox);
    let made = overseer::partsums::summaries_made();
    let nodes = open_fresh(&sandbox);
    assert!(overseer::partsums::summaries_made() > made, "each start summarises the parts again");
    assert_eq!(totals(&nodes), [int(20), int(12), int(2)]);
    assert!(!sandbox.0.join("cache").exists());
}

#[test]
fn a_summary_read_from_disk_is_marked_as_used() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("touched", true);
    open_fresh(&sandbox);
    assert!(summaries_on_disk(&sandbox) > 0);
    let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 24 * 3600);
    let files: Vec<_> = std::fs::read_dir(sandbox.0.join("cache/sums"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    for file in &files {
        std::fs::File::options().write(true).open(file).unwrap().set_modified(long_ago).unwrap();
    }
    open_fresh(&sandbox);
    overseer::document_disk::settled();
    let touched = files
        .iter()
        .filter(|file| std::fs::metadata(file).unwrap().modified().unwrap() > long_ago)
        .count();
    assert_eq!(touched, files.len(), "a trim keeps what was read, and lets go of what was not");
}

#[test]
fn every_chain_over_a_part_is_kept_in_one_file() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("one_file", true);
    open_fresh(&sandbox);
    assert_eq!(summaries_on_disk(&sandbox), 2, "one file for each of the two parts outside the window");
}

#[test]
fn a_new_chain_over_a_part_joins_its_file_and_keeps_the_others() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("new_chain", true);
    open_fresh(&sandbox);
    let document = sandbox.0.join("documents/log.os");
    let more = DOCUMENT.replace(
        "    list Items",
        "    int most (mutable=false) = $(History.map(|x| x/points).max())\n\n    list Items",
    );
    std::fs::write(&document, more).unwrap();
    let made = overseer::partsums::summaries_made();
    let nodes = open_fresh(&sandbox);
    assert!(overseer::partsums::summaries_made() > made, "the new chain summarises the parts");
    assert_eq!(value(&nodes, "log/most"), int(8));

    let made = overseer::partsums::summaries_made();
    let nodes = open_fresh(&sandbox);
    assert_eq!(overseer::partsums::summaries_made(), made, "neither the new chain nor the old ones are made again");
    assert_eq!(totals(&nodes), [int(20), int(12), int(2)]);
    assert_eq!(value(&nodes, "log/most"), int(8));
    assert_eq!(summaries_on_disk(&sandbox), 2);
}
