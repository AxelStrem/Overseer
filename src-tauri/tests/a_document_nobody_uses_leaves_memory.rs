//! A document nobody has used for a while leaves memory, and a page left open on it goes on
//! catching up by its changes.
//!
//! A large cache serves whoever is using the server; in the quiet hours nobody is, and what it holds
//! is billed all the same. So what nobody has asked for within `OVERSEER_IDLE_MINUTES` goes to
//! disk, as what is let go for want of room does, and is read back from there when next asked for.
//! The trees kept for pages to catch up from go too: a page that catches up from one that is gone
//! has it worked out again from the text it was handed, rather than being sent the whole document.
//!
//!   cargo test --test a_document_nobody_uses_leaves_memory

use overseer::types::*;
use overseer::{app_api, delta, document_cache, document_disk};
use std::path::PathBuf;
use std::time::Duration;

const DOCUMENT: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string id = ""
            int qty = 0
        }
    }
    list Rows (entry=<Row>, key="id") {
        - {
            - id = "b"
            - qty = 2
        }
        - {
            - id = "c"
            - qty = 3
        }
    }
    int total = $(Rows.sum(|x| x/qty))
}
"#;

/// The cache, the disk and the watchers are all process-wide.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox {
    root: PathBuf,
    path: String,
    cache: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        document_disk::settled();
        document_disk::set_override(None);
        document_cache::set_budget_override(None);
        app_api::forget_baseline();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A document in a folder of its own, and a folder for what leaves memory - or none, `on_disk`
/// false, for a server that keeps nothing there.
fn sandbox(tag: &str, on_disk: bool) -> Sandbox {
    let root = std::env::temp_dir().join(format!("overseer_idle_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    let cache = root.join("cache");
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    document_cache::set_budget_override(Some(512 * 1024 * 1024));
    document_disk::set_override(Some(on_disk.then(|| (cache.clone(), 512 * 1024 * 1024))));
    Sandbox { root, path: file.to_string_lossy().to_string(), cache }
}

fn files_kept(sandbox: &Sandbox) -> usize {
    document_disk::settled();
    std::fs::read_dir(&sandbox.cache).map_or(0, |listing| {
        listing.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")).count()
    })
}

/// The bot sets a row's quantity, naming the row by its key.
fn sets(path: &str, session: &str, entry: &str, qty: i64) -> app_api::ResolvedUpdate {
    let nodes = app_api::load_document(std::fs::read_to_string(path).unwrap()).unwrap();
    let mut field = overseer::addressing::name_path(&nodes, &format!("t/Rows/[{}]", entry)).expect("no such row");
    field.push("qty".into());
    let w: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
        "node_path": field, "value": OverseerValue::Integer(qty)
    }))
    .unwrap();
    app_api::write_values_at(path, "d.os", vec![w], session).expect("the write")
}

fn catches_up(path: &str, showing: &str) -> app_api::ResolvedUpdate {
    let _showing = app_api::describing_for(Some(app_api::version_of(showing)));
    app_api::catch_up_at(path, "d.os", "page").expect("the catch-up")
}

/// What the page shows of a document: each node's name and value, and its children.
fn shown(nodes: &[OverseerNode]) -> serde_json::Value {
    serde_json::Value::Array(
        nodes
            .iter()
            .map(|n| {
                let value = n.parameters.get("_computed_value").or_else(|| n.parameters.get("value"));
                serde_json::json!({
                    "name": n.name,
                    "value": serde_json::to_value(value).unwrap(),
                    "children": shown(&n.children),
                })
            })
            .collect(),
    )
}

fn taken(mut held: Vec<OverseerNode>, update: app_api::ResolvedUpdate) -> Vec<OverseerNode> {
    assert!(update.nodes.is_none(), "the page was sent the whole document");
    let changes = update.changes.expect("the page was told nothing");
    delta::apply(&mut held, changes);
    held
}

fn the_file(path: &str) -> Vec<OverseerNode> {
    app_api::forget_baseline();
    app_api::load_document(std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn a_document_unused_past_the_limit_goes_to_disk_and_comes_back_the_same() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("disk", true);
    let opened = app_api::load_document(DOCUMENT.to_string()).expect("open");
    assert!(document_cache::held().0 > 0, "the document was not held to begin with");

    assert_eq!(document_cache::let_go_idle(Duration::from_secs(3600)), 0, "a document just used was let go");
    assert!(document_cache::held().0 > 0);

    assert!(document_cache::let_go_idle(Duration::ZERO) > 0, "nothing was let go");
    assert_eq!(document_cache::held().0, 0, "the document is still in memory");
    assert_eq!(files_kept(&sandbox), 1, "the document was not written to disk");

    let again = app_api::load_document(DOCUMENT.to_string()).expect("open again");
    assert_eq!(shown(&again), shown(&opened), "what came back is not what was let go");
    assert_eq!(files_kept(&sandbox), 0, "it was worked out again rather than read back");
}

#[test]
fn the_parts_mounts_and_bases_unused_past_the_limit_go_with_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _sandbox = sandbox("everywhere", true);
    app_api::load_document(DOCUMENT.to_string()).expect("open");
    document_cache::let_go_idle_everywhere(Duration::ZERO);
    assert_eq!(document_cache::held().0, 0);
    assert_eq!(overseer::parts::read_parts_held().0, 0);
    assert_eq!(overseer::actions::mounted_held().0, 0);
    assert_eq!(overseer::bases::parsed_held().0, 0);
    assert_eq!(document_cache::superseded_held().0, 0);
}

/// A page open on the document, written from elsewhere after a quiet stretch: what it shows is
/// read back from disk for the write, and kept for it to catch up from.
#[test]
fn a_page_left_open_catches_up_by_its_changes_after_the_document_left_memory() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("open", true);
    let (_, _listening) = overseer::watch::listen(std::path::Path::new(&sandbox.path));
    let held = app_api::load_document_for(DOCUMENT.to_string(), "d.os", "page").expect("open");
    document_cache::let_go_idle_everywhere(Duration::ZERO);
    document_disk::settled();

    sets(&sandbox.path, "bot", "c", 7);
    let now = taken(held, catches_up(&sandbox.path, DOCUMENT));
    assert_eq!(shown(&now), shown(&the_file(&sandbox.path)), "the page does not show what the file holds");
}

/// A page that was hidden when the bot wrote, and stayed hidden through a quiet stretch: the tree
/// kept for it to catch up from went with the rest, and nothing on disk is for the text it shows.
/// It is worked out again from that text, with no disk at all as well.
#[test]
fn a_page_whose_tree_left_memory_catches_up_from_the_text_it_was_handed() {
    for on_disk in [true, false] {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let sandbox = sandbox(if on_disk { "hidden_disk" } else { "hidden" }, on_disk);
        let (_, _listening) = overseer::watch::listen(std::path::Path::new(&sandbox.path));
        let held = app_api::load_document_for(DOCUMENT.to_string(), "d.os", "page").expect("open");
        sets(&sandbox.path, "bot", "c", 7);
        document_cache::let_go_idle_everywhere(Duration::ZERO);
        document_disk::settled();
        assert_eq!(document_cache::superseded_held().0, 0, "the tree kept for the page is still held");

        let now = taken(held, catches_up(&sandbox.path, DOCUMENT));
        assert_eq!(
            shown(&now),
            shown(&the_file(&sandbox.path)),
            "the page does not show what the file holds (disk {})",
            on_disk
        );
    }
}
