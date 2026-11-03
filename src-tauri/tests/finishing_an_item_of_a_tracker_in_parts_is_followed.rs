//! Finishing an item of a project tracker, and reopening it, is followed through the graph rather
//! than working the tracker out whole.
//!
//! A tracker's History shows its newest twenty and keeps the rest in weekly parts. Finishing an
//! item appends it to History and takes it out of Items. History's window used to stop the graph
//! from following that append, so the press worked the tracker out whole three times over, which
//! took one to two seconds on the real trackers. What the window shows is now decided again for
//! the one list. The record still goes to its week's part, and what is drawn agrees with a fresh
//! open of the files.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::resolver::times_worked_out_whole;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};

const EXAMPLE: &str = include_str!("../../examples/projects/project.os");

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        FormulaEvaluator::set_time_override(None);
        overseer::document_disk::set_override(None);
    }
}

/// Twenty-five records finished in the week before the press, more than History's window shows.
fn last_week() -> String {
    let mut text = String::from("list History {\n");
    for n in 1..=25 {
        text.push_str(&format!(
            "    - {{\n        - finished_at = \"2026-09-{:02}T10:00:00Z\"\n        - title = \"Old {}\"\n        - handle = \"old{}\"\n        - points = 1\n    }}\n",
            28 + n % 3,
            n,
            n
        ));
    }
    text.push_str("}\n");
    text
}

/// The example project, its History in parts, opened as the server opens it.
fn a_tracker(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_tracker_followed_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("p.parts/History")).unwrap();
    std::fs::write(root.join("p.os"), EXAMPLE).unwrap();
    std::fs::write(root.join("p.parts/History/2026-W40.os"), last_week()).unwrap();
    overseer::document_disk::set_override(Some(None));
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-06T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

/// Pick a stage on an item, as the page does: the value written and its `on change` run as one.
fn pick(documents: &DocumentRoot, handle: &str, stage: &str) {
    let nodes = documents.open("p.os").expect("open");
    let path = overseer::addressing::name_path(&nodes, &format!("project/Items/[{}]/stage", handle))
        .unwrap_or_else(|| panic!("no stage on {}", handle));
    documents
        .command(
            Some("p.os"),
            "run_overseer_event",
            &serde_json::json!({
                "node_path": path,
                "event_name": "change",
                "writing": [{ "node_path": path, "value": { "String": stage } }],
            }),
        )
        .expect("the pick");
}

fn on_disk(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).unwrap_or_default()
}

/// Every worked-out value of what is drawn, by address: the entries in view and nothing of those
/// out of it. The server has read more of History's parts than a fresh open does - the first write
/// read the old weeks whose records the document held, to put them in their parts - so it holds
/// more entries out of view, and names the ones in view by other places in the list. Neither is
/// drawn, and a keyed address names the same entry either way.
fn worked_out(nodes: &[OverseerNode]) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut hidden: Vec<String> = Vec::new();
    overseer::addressing::walk(nodes, &mut |address, node| {
        if hidden.iter().any(|h| address == h || address.starts_with(&format!("{}/", h))) {
            return;
        }
        if overseer::resolver::out_of_view(node) {
            hidden.push(address.to_string());
            return;
        }
        if !address.ends_with(']') {
            out.insert(address.to_string(), node.name.clone());
        }
        for (k, v) in &node.parameters {
            // A timestamp worked out from the clock is the clock's, read whenever it was.
            if k.starts_with("_computed_") && !address.ends_with("/touched_at") {
                // A time worked out from one written by an action is held as a timestamp, and
                // the same time read from the file as text: what is drawn is the time either way.
                let said = match v {
                    OverseerValue::Timestamp(t) | OverseerValue::String(t) => t.clone(),
                    other => format!("{:?}", other),
                };
                out.insert(format!("{}#{}", address, k), said);
            }
        }
    });
    out
}

fn agrees_with_a_fresh_open(sandbox: &Sandbox, held: &[OverseerNode]) {
    app_api::forget_baseline();
    app_api::forget_dependencies();
    let fresh = DocumentRoot::new(&sandbox.0).unwrap().open("p.os").expect("a fresh open");
    let (held, fresh) = (worked_out(held), worked_out(&fresh));
    let differ: Vec<_> = held
        .keys()
        .chain(fresh.keys())
        .filter(|k| held.get(*k) != fresh.get(*k))
        .map(|k| format!("{}: {:?} / fresh {:?}", k, held.get(k), fresh.get(k)))
        .collect();
    assert!(differ.is_empty(), "the server and a fresh open disagree:\n{}", differ.join("\n"));
}

#[test]
fn finishing_an_item_goes_to_its_part_without_working_the_tracker_out_whole() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = a_tracker("finish");
    // The first write moves the records the document held into their parts.
    pick(&documents, "rows", "finished");
    assert!(!on_disk(&sandbox.0, "p.os").contains("handle = \"roundtrip\""), "History is still in the document");

    let whole = times_worked_out_whole();
    pick(&documents, "favicon", "finished");
    // One with a conversation, which the record copies.
    pick(&documents, "brace", "finished");
    assert_eq!(times_worked_out_whole(), whole, "worked out whole");

    let this_week = on_disk(&sandbox.0, "p.parts/History/2026-W41.os");
    assert!(this_week.contains("handle = \"favicon\"") && this_week.contains("handle = \"brace\""), "{}", this_week);
    let document = on_disk(&sandbox.0, "p.os");
    assert!(!document.contains("handle = \"favicon\"") && !document.contains("handle = \"brace\""));

    let held = documents.open("p.os").expect("open");
    let in_view = |held: &[OverseerNode]| {
        overseer::addressing::find(held, "project/History")
            .unwrap()
            .children
            .iter()
            .filter(|e| !overseer::resolver::out_of_view(e))
            .count()
    };
    assert_eq!(in_view(&held), 20, "History shows its window");
    assert!(!overseer::resolver::out_of_view(overseer::addressing::find(&held, "project/History/[brace]").unwrap()));
    agrees_with_a_fresh_open(&sandbox, &held);
}

#[test]
fn reopening_a_record_brings_the_next_one_into_view_without_working_the_tracker_out_whole() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = a_tracker("reopen");
    pick(&documents, "rows", "finished");
    pick(&documents, "favicon", "finished");

    let whole = times_worked_out_whole();
    documents.run_event("p.os", "project/History/[favicon]/reopen", "click").expect("the reopen");
    assert_eq!(times_worked_out_whole(), whole, "worked out whole");

    assert!(!on_disk(&sandbox.0, "p.parts/History/2026-W41.os").contains("handle = \"favicon\""));
    let held = documents.open("p.os").expect("open");
    assert!(overseer::addressing::find(&held, "project/Items/[favicon]").is_some(), "not back on the open list");
    agrees_with_a_fresh_open(&sandbox, &held);
}
