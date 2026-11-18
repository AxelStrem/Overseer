//! A server without the summaries of a history's parts parses every part once to make them. Once
//! the document is worked out, the parts read only for that are let go: what stays held is the
//! parts the window draws, and the totals are the same.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::PathBuf;

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

/// A week's part: one entry under `p` worth `points`, done on `day` of the week before November.
fn week(handle: &str, day: &str, points: i64) -> String {
    format!(
        "list History {{\n    - {{\n        - handle = \"{handle}\"\n        - parent = \"p\"\n        - done_at = \"{day}T08:00:00Z\"\n        - points = {points}\n    }}\n}}\n"
    )
}

const THIS_WEEK: &str = r#"list History {
    - {
        - handle = "now"
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

/// A history in five weekly parts, of which the window draws the newest - and a disk cache for
/// the summaries, or none for `cache` false.
fn sandbox(tag: &str, cache: bool) -> Sandbox {
    let root = std::env::temp_dir().join(format!("overseer_sumletgo_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let documents = root.join("documents");
    let parts = documents.join("log.parts/History");
    std::fs::create_dir_all(&parts).unwrap();
    std::fs::write(documents.join("log.os"), DOCUMENT).unwrap();
    for (n, (part, day)) in
        [("2026-W41", "2026-10-06"), ("2026-W42", "2026-10-13"), ("2026-W43", "2026-10-20"), ("2026-W44", "2026-10-27")]
            .iter()
            .enumerate()
    {
        std::fs::write(parts.join(format!("{part}.os")), week(&format!("w{n}"), day, n as i64 + 1)).unwrap();
    }
    std::fs::write(parts.join("2026-W45.os"), THIS_WEEK).unwrap();
    let setting = cache.then(|| (root.join("cache"), 64 * 1024 * 1024));
    overseer::document_disk::set_override(Some(setting));
    Sandbox(root)
}

/// The document opened as by a server that has just started: no summary and no document held.
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

const TOTALS: [i64; 3] = [1 + 2 + 3 + 4 + 5, 1 + 2 + 3 + 4 + 5, 4];

fn check(sandbox: &Sandbox, opened: &str) {
    let held_before = overseer::parts::read_parts_held().0;
    let made = overseer::partsums::summaries_made();
    let nodes = open_fresh(sandbox);
    assert!(overseer::partsums::summaries_made() > made, "{opened}: the old parts are summarised");
    assert_eq!(totals(&nodes), TOTALS.map(int), "{opened}");
    let held = overseer::parts::read_parts_held().0;
    // The part the window draws stays, for the writes that read it again; the four summarised go.
    assert_eq!(held, held_before + 1, "{opened}: parts held after the open, with {held_before} before");
}

#[test]
fn a_first_open_holds_only_the_parts_its_window_draws() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // The summaries made from a parse of each part, and written to disk.
    let disk = sandbox("disk", true);
    check(&disk, "with the disk cache");
    // Read back from disk by the next server, which reads no old part at all.
    let made = overseer::partsums::summaries_made();
    let nodes = open_fresh(&disk);
    assert_eq!(overseer::partsums::summaries_made(), made, "no part is summarised again");
    assert_eq!(totals(&nodes), TOTALS.map(int));
    drop(disk);

    // With no disk cache, every open of a fresh server summarises the parts again.
    let none = sandbox("nodisk", false);
    check(&none, "without the disk cache");
}

#[test]
fn parsing_a_history_for_its_summaries_does_not_push_out_the_parts_the_window_draws() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Room for one part only: the summaries' parses are held apart and push out none of these.
    overseer::bounded::set_limit_override(Some(1));
    let tight = sandbox("tight", false);
    let nodes = open_fresh(&tight);
    let held = overseer::parts::read_parts_held().0;
    overseer::bounded::set_limit_override(None);
    assert_eq!(totals(&nodes), TOTALS.map(int));
    assert_eq!(held, 1, "the part the window draws is held");
}
