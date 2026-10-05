//! The bot's writes start from the document already worked out, as the page's do.
//!
//! Every write through the server's own doors - set, append, remove, press - used to work the
//! whole document out before the work and again after it, and never from the cache: the address
//! a write is about is held in view in case a window left it out, and a document resolved with
//! something held in view is not the one anyone else is shown, so it was neither taken from the
//! cache nor put in it. Logging a meal from a message paid two full resolves of the food tracker.
//!
//! Now, when the document is held for exactly the text in the file and what the write is about is
//! in view there, the write starts from it and works out what it reaches - the graph asked for a
//! value, and carried through an entry made or taken out, see `a_change_of_shape_is_followed` - and
//! what comes out is kept for the text written, so the next write, or the page reopening it,
//! starts from that. A write to a day the window leaves out still takes the long way, which is the
//! one that can reach it.

use overseer::server::DocumentRoot;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Meal (layout="horizontal") {
            float grams = 0
            float calories = $(grams * 2)
        }
        div Day (layout="vertical") {
            date day = "2026-01-01"
            list intake (entry=<Meal>) {
            }
            float total = $(intake.map(|x| x/calories).sum())
        }
    }

    list Days (entry=<Day>, key="day", window=1, sort_by=$(|x| 0 - millis_since_epoch(x/day))) {
        - {
            - day = "2026-09-25"
            list intake {
                - {
                    - grams = 100
                }
            }
        }
        - {
            - day = "2026-09-26"
            list intake {
                - {
                    - grams = 50
                }
            }
        }
    }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

struct Sandbox {
    root: std::path::PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A root holding the document, opened once the way the bot's reads open it, so it is held.
fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_botwrites_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    documents.open("d.os").expect("open");
    (Sandbox { root }, documents)
}

fn text(sandbox: &Sandbox) -> String {
    std::fs::read_to_string(sandbox.root.join("d.os")).unwrap()
}

/// What the cache holds for this folder: the same text elsewhere is another document - see
/// `a_document_in_two_folders_is_two_documents`.
fn asked_from<T>(documents: &DocumentRoot, ask: impl FnOnce() -> T) -> T {
    overseer::docmgr::manager::DocumentManager::with_document(Some(documents.path().to_path_buf()), ask)
}

fn number(node: &OverseerNode) -> Option<f64> {
    match node.parameters.get("_computed_value") {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

fn value(nodes: &[OverseerNode], address: &str) -> Option<f64> {
    number(overseer::addressing::find(nodes, address)?)
}

/// Every worked-out value, by address.
fn computed(nodes: &[OverseerNode]) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    overseer::addressing::walk(nodes, &mut |address, node| {
        for (k, v) in &node.parameters {
            if k.starts_with("_computed_") {
                out.insert(format!("{}#{}", address, k), format!("{:?}", v));
            }
        }
    });
    out
}

/// The document in the file, worked out from nothing.
fn fresh(sandbox: &Sandbox) -> Vec<OverseerNode> {
    let (_rest, mut nodes) = overseer::parser::parse_document(&text(sandbox)).expect("parse");
    overseer::resolver::resolve_document(&mut nodes);
    nodes
}

const TODAY: &str = "t/Days/[2026-09-26]";

#[test]
fn a_value_the_bot_writes_reaches_what_reads_it() {
    serialised(|| {
        let (sandbox, documents) = sandbox("set");
        let outcome = documents
            .set_at("d.os", &format!("{}/intake/Meal__1/grams", TODAY), OverseerValue::Float(80.0))
            .expect("the write");
        assert_eq!(number(&outcome.node), None, "a field holding a plain value");
        let now = documents.open("d.os").expect("reopen");
        assert_eq!(value(&now, &format!("{}/total", TODAY)), Some(160.0));
        assert_eq!(computed(&now), computed(&fresh(&sandbox)), "the write and a fresh open disagree");
    });
}

#[test]
fn what_the_bot_wrote_is_held_for_the_next_write() {
    // The long way held nothing, so the page reopening what the bot had just changed paid a full
    // resolve, and so did the bot's next write.
    serialised(|| {
        let (sandbox, documents) = sandbox("held");
        documents
            .set_at("d.os", &format!("{}/intake/Meal__1/grams", TODAY), OverseerValue::Float(80.0))
            .expect("the write");
        let written = text(&sandbox);
        assert!(
            asked_from(&documents, || overseer::document_cache::nodes_for(&written)).is_some(),
            "nothing held for the text written"
        );
        assert!(
            asked_from(&documents, || overseer::document_cache::graph_for(&written)).is_some(),
            "no graph held for the text written"
        );
    });
}

#[test]
fn a_meal_the_bot_adds_can_be_corrected() {
    serialised(|| {
        let (sandbox, documents) = sandbox("append");
        let fields = [("grams".to_string(), OverseerValue::Float(20.0))].into_iter().collect();
        let outcome = documents.append_at("d.os", &format!("{}/intake", TODAY), &fields).expect("the append");
        let added = outcome.child_addresses.last().expect("the new meal").clone();
        let now = documents.open("d.os").expect("reopen");
        assert_eq!(value(&now, &format!("{}/total", TODAY)), Some(140.0));

        documents.set_at("d.os", &format!("{}/grams", added), OverseerValue::Float(30.0)).expect("the correction");
        let now = documents.open("d.os").expect("reopen");
        assert_eq!(value(&now, &format!("{}/calories", added)), Some(60.0), "the new meal kept its old calories");
        assert_eq!(value(&now, &format!("{}/total", TODAY)), Some(160.0), "the day kept its old total");
        assert_eq!(computed(&now), computed(&fresh(&sandbox)));
    });
}

#[test]
fn a_day_the_window_leaves_out_is_still_written() {
    // Yesterday is out of view: the window shows one day. The quick way cannot reach it, and the
    // long way still does.
    serialised(|| {
        let (sandbox, documents) = sandbox("old");
        documents
            .set_at("d.os", "t/Days/[2026-09-25]/intake/Meal__1/grams", OverseerValue::Float(10.0))
            .expect("the write");
        assert!(text(&sandbox).contains("- grams = 10"), "the write did not land: {}", text(&sandbox));
        let now = documents.open("d.os").expect("reopen");
        assert_eq!(computed(&now), computed(&fresh(&sandbox)));
    });
}

#[test]
fn after_a_removal_the_entries_are_named_as_their_text_names_them() {
    // Named by place when they are made, and never again: the document held after a removal kept
    // the survivors' old names, while its text names them afresh - so an address read before the
    // removal still reached the same meal from the held document and a different one from the
    // text. The text is the authority. Found by the bot's own test of exactly that.
    serialised(|| {
        let (sandbox, documents) = sandbox("remove");
        let fields = [("grams".to_string(), OverseerValue::Float(20.0))].into_iter().collect();
        documents.append_at("d.os", &format!("{}/intake", TODAY), &fields).expect("the append");
        documents
            .remove_at("d.os", &format!("{}/intake/Meal__1", TODAY), &Default::default())
            .expect("the removal");
        let now = documents.open("d.os").expect("reopen");
        let named = |nodes: &[OverseerNode]| overseer::addressing::addresses(nodes);
        assert_eq!(named(&now), named(&fresh(&sandbox)), "held and parsed name the entries differently");
        assert_eq!(value(&now, &format!("{}/intake/Meal__1/calories", TODAY)), Some(40.0));
        assert_eq!(computed(&now), computed(&fresh(&sandbox)));
    });
}

#[test]
fn a_write_refused_before_it_changed_anything_leaves_the_document_held() {
    // A write takes the document held for its text rather than copying it, and puts it back when
    // it is refused before anything ran - the bot's own checks refuse a good many writes. Let go
    // instead, the next write would work the whole document out again.
    serialised(|| {
        let (sandbox, documents) = sandbox("refused");
        let fields = [("no_such_field".to_string(), OverseerValue::Float(1.0))].into_iter().collect();
        assert!(documents.append_at("d.os", &format!("{}/intake", TODAY), &fields).is_err());
        assert!(
            asked_from(&documents, || overseer::document_cache::nodes_for(&text(&sandbox))).is_some(),
            "the refused write let the document go"
        );
        let whole = overseer::resolver::times_worked_out_whole();
        documents
            .set_at("d.os", &format!("{}/intake/Meal__1/grams", TODAY), OverseerValue::Float(80.0))
            .expect("the next write");
        assert_eq!(overseer::resolver::times_worked_out_whole(), whole, "the next write worked it out whole");
    });
}
