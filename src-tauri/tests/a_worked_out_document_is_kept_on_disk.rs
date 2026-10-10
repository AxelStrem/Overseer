//! What the cache lets go of is written to disk, and read back rather than worked out again.
//!
//! The server's cache holds about two of the documents in use, and working out one that was let go
//! again was most of what moving between them cost. Read back from a file it is about a third of
//! that. What matters more than the time is that what comes back is the same document: the same values, the same file when it is written, and never a document worked
//! out against a mounted file that has since changed.
//!
//! The memory budget is set to hold one copy of the tasks example and a half, so opening two other
//! copies of it, in folders of their own, pushes out whatever else was held - as moving between the
//! documents does on the server.

use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::source_registry::SourceRegistry;
use overseer::types::*;
use overseer::{app_api, document_cache, document_disk, resolver};
use std::path::{Path, PathBuf};

const HOST: &str = r#"tab t (label="T", mutable=true) {
    mount CATALOG (hidden=true, lazy=false, mutable=false, source="side.os/s/Things") { }

    div (hidden=true) {
        div Use (layout="horizontal") {
            string thing = ""
            float worth = $(/t/CATALOG/Things.filter(|x| x/handle == ../thing).map(|x| x/worth).sum())
        }
    }

    list uses (entry=<Use>) {
        - {
            - thing = "a"
        }
    }
}
"#;

const SIDE: &str = r#"tab s (label="S", mutable=true) {
    div (hidden=true) {
        div Thing (layout="horizontal") {
            string handle = ""
            int worth = 0
        }
    }

    list Things (entry=<Thing>, key="handle") {
        - {
            - handle = "a"
            - worth = 1
        }
    }
}
"#;

const TASKS: &str = "../examples/tasks/tasks.os";

/// The budgets, the cache and the registry are all process-wide.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox {
    root: PathBuf,
    documents: PathBuf,
    cache: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        document_disk::settled();
        document_disk::set_override(None);
        document_cache::set_budget_override(None);
        app_api::forget_baseline();
        app_api::forget_dependencies();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sandbox(tag: &str) -> Sandbox {
    let root = std::env::temp_dir().join(format!("overseer_on_disk_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let documents = root.join("documents");
    let cache = root.join("cache");
    for folder in ["documents", "other", "another"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::copy(TASKS, root.join(folder).join("tasks.os")).unwrap();
    }
    std::fs::write(documents.join("host.os"), HOST).unwrap();
    std::fs::write(documents.join("side.os"), SIDE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    // Measured rather than guessed, so the documents can grow without the tests going wrong.
    document_cache::set_budget_override(Some(512 * 1024 * 1024));
    document_disk::set_override(Some(None));
    let documents = documents.canonicalize().unwrap();
    open(&root.join("another"), "tasks.os");
    let one = document_cache::held().1;
    app_api::forget_baseline();
    app_api::forget_dependencies();
    document_cache::set_budget_override(Some(one * 3 / 2));
    document_disk::set_override(Some(Some((cache.clone(), 512 * 1024 * 1024))));
    Sandbox { root, documents, cache }
}

/// Opened as the server opens it, in its own folder.
fn open(folder: &Path, name: &str) -> Vec<OverseerNode> {
    let folder = folder.canonicalize().unwrap();
    let text = std::fs::read_to_string(folder.join(name)).unwrap();
    DocumentManager::with_document(Some(folder), || {
        app_api::load_document_with_dependencies(text).expect("open")
    })
}

/// Open two other copies of the tasks, which leaves room in memory for nothing else.
fn push_out(sandbox: &Sandbox) {
    open(&sandbox.root.join("other"), "tasks.os");
    open(&sandbox.root.join("another"), "tasks.os");
    document_disk::settled();
}

/// The file the document in this folder was written to, if it was.
fn kept(folder: &Path, name: &str) -> Option<PathBuf> {
    document_disk::settled();
    let folder = folder.canonicalize().unwrap();
    let text = std::fs::read_to_string(folder.join(name)).unwrap();
    document_disk::where_kept(&text, &Some(folder))
}

/// A document with the ids of its snapshots taken off. They name a parse in the registry, and a
/// document read back is given a parse of its own - see `SourceRegistry::restore`.
fn without_ids(nodes: &[OverseerNode]) -> Vec<OverseerNode> {
    fn strip(nodes: &mut [OverseerNode]) {
        for node in nodes {
            node.source_id = None;
            strip(&mut node.children);
        }
    }
    let mut nodes = nodes.to_vec();
    strip(&mut nodes);
    nodes
}

fn worth(nodes: &[OverseerNode], address: &str) -> Option<f64> {
    match overseer::addressing::find(nodes, address)?.parameters.get("_computed_value") {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

#[test]
fn a_document_let_go_comes_back_from_disk_as_it_was() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("same");
    let first = open(&sandbox.documents, "tasks.os");
    assert!(kept(&sandbox.documents, "tasks.os").is_none(), "written down while still held");
    push_out(&sandbox);
    assert!(kept(&sandbox.documents, "tasks.os").is_some(), "what was let go was not written down");

    // As after a restart: nothing the parse registered is left.
    SourceRegistry::reset();
    let whole = resolver::times_worked_out_whole();
    let again = open(&sandbox.documents, "tasks.os");
    assert_eq!(
        resolver::times_worked_out_whole(),
        whole,
        "the document was worked out again rather than read back"
    );
    assert!(kept(&sandbox.documents, "tasks.os").is_none(), "read back and still on disk");
    assert_eq!(without_ids(&again), without_ids(&first), "what came back is not what was let go");
    assert_eq!(
        serde_json::to_string(&without_ids(&again)).unwrap(),
        serde_json::to_string(&without_ids(&first)).unwrap(),
        "the same, but not in the same order"
    );
    let text = std::fs::read_to_string(sandbox.documents.join("tasks.os")).unwrap();
    assert_eq!(
        OverseerFileHandler::serialize_nodes(&again).unwrap(),
        text,
        "written out after coming back from disk, the document is not the file it came from"
    );
}

#[test]
fn a_write_to_a_document_read_back_from_disk_writes_what_it_always_did() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let header = || OverseerValue::String("# Today".into());
    // The same write made twice: once to a document held in memory, and once to one let go and
    // read back after a restart.
    let expected = {
        let sandbox = sandbox("write_held");
        let root = DocumentRoot::new(&sandbox.documents).unwrap();
        root.open("tasks.os").expect("open");
        root.set_at("tasks.os", "tasks/header", header()).expect("a write");
        std::fs::read_to_string(sandbox.documents.join("tasks.os")).unwrap()
    };
    let sandbox = sandbox("write_disk");
    let root = DocumentRoot::new(&sandbox.documents).unwrap();
    root.open("tasks.os").expect("open");
    push_out(&sandbox);
    assert!(kept(&sandbox.documents, "tasks.os").is_some(), "what was let go was not written down");
    SourceRegistry::reset();
    let whole = resolver::times_worked_out_whole();
    root.set_at("tasks.os", "tasks/header", header()).expect("a write");
    assert_eq!(
        resolver::times_worked_out_whole(),
        whole,
        "the write worked the document out again rather than starting from the one on disk"
    );
    let written = std::fs::read_to_string(sandbox.documents.join("tasks.os")).unwrap();
    assert!(written.contains("# Today"), "the write did not reach the file");
    assert_eq!(written, expected, "starting from disk, the write wrote something else");
}

#[test]
fn a_change_to_a_mounted_file_is_not_read_past() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("mount");
    let before = open(&sandbox.documents, "host.os");
    assert_eq!(worth(&before, "t/uses/Use__1/worth"), Some(1.0));
    push_out(&sandbox);
    assert!(kept(&sandbox.documents, "host.os").is_some(), "what was let go was not written down");

    // A different length, so the stamp moves however coarse the clock on the file is.
    std::fs::write(sandbox.documents.join("side.os"), SIDE.replace("- worth = 1", "- worth = 17")).unwrap();
    let after = open(&sandbox.documents, "host.os");
    assert_eq!(
        worth(&after, "t/uses/Use__1/worth"),
        Some(17.0),
        "handed back as worked out against the mounted file before it changed"
    );
}

#[test]
fn the_same_text_in_another_folder_is_not_read_back() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("folder");
    open(&sandbox.documents, "tasks.os");
    push_out(&sandbox);
    assert!(kept(&sandbox.documents, "tasks.os").is_some());

    let elsewhere = sandbox.root.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::copy(TASKS, elsewhere.join("tasks.os")).unwrap();
    let whole = resolver::times_worked_out_whole();
    open(&elsewhere, "tasks.os");
    assert!(
        resolver::times_worked_out_whole() > whole,
        "a document worked out in one folder was handed out in another"
    );
    assert!(kept(&sandbox.documents, "tasks.os").is_some(), "another folder's file was taken");
}

#[test]
fn a_damaged_file_is_worked_out_again_and_not_kept() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("damaged");
    let first = open(&sandbox.documents, "tasks.os");
    push_out(&sandbox);
    let file = kept(&sandbox.documents, "tasks.os").expect("written down");
    let bytes = std::fs::read(&file).unwrap();
    std::fs::write(&file, &bytes[..bytes.len() / 2]).unwrap();

    let whole = resolver::times_worked_out_whole();
    let again = open(&sandbox.documents, "tasks.os");
    assert!(resolver::times_worked_out_whole() > whole, "half a file was read as a document");
    assert_eq!(without_ids(&again), without_ids(&first));
    assert!(kept(&sandbox.documents, "tasks.os").is_none(), "the damaged file was left behind");
}

#[test]
fn the_files_are_kept_within_their_budget() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = sandbox("budget");
    open(&sandbox.documents, "tasks.os");
    push_out(&sandbox);
    let file = kept(&sandbox.documents, "tasks.os").expect("written down");
    let one = std::fs::metadata(&file).unwrap().len() as usize;

    // Room for one file of the tasks and not two: the copy let go next is written, and the one
    // written before it goes.
    document_disk::set_override(Some(Some((sandbox.cache.clone(), one * 3 / 2))));
    std::thread::sleep(std::time::Duration::from_millis(50));
    open(&sandbox.documents, "host.os");
    push_out(&sandbox);
    assert!(kept(&sandbox.documents, "tasks.os").is_none(), "the budget was not kept");
    let held: Vec<_> = std::fs::read_dir(&sandbox.cache)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .map(|e| e.metadata().unwrap().len() as usize)
        .collect();
    assert!(!held.is_empty(), "nothing was kept at all");
    assert!(held.iter().sum::<usize>() <= one * 3 / 2, "the budget was not kept");
}
