//! An item finished into a weekly part of a tracker's History is written at its depth in the part:
//! `- {` one unit in, its fields two, and everything inside them below that.
//!
//! In the tracker the same item sits two units deeper, inside `Items`. Weeks written by the server
//! came out with the item's lines at their old depth, or with only its first line moved and the
//! rest written with the item's old indent taken for the file's unit. The parser reads either, but
//! the files no longer look like the rest of the documents, and line-based readers lose entries.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::server::DocumentRoot;
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

const WEEK: &str = "p.parts/History/2026-W41.os";

/// The example project, its History in parts, opened as the server opens it. `week` is what the
/// week the presses finish into already holds, if anything.
fn a_tracker(tag: &str, week: Option<&str>) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_partindent_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("p.parts/History")).unwrap();
    std::fs::write(root.join("p.os"), EXAMPLE).unwrap();
    if let Some(week) = week {
        std::fs::write(root.join(WEEK), week).unwrap();
    }
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

/// Every line of a document that is not at four spaces for each brace it stands inside, a closing
/// brace counting as outside its own block. A line that begins inside a string is the string's.
fn out_of_place(text: &str) -> Vec<String> {
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut wrong = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let body = line.trim_start();
        if !in_string && !body.is_empty() {
            let wanted = if body.starts_with('}') { depth.saturating_sub(1) } else { depth };
            let indent = line.len() - body.len();
            if indent != wanted * 4 {
                wrong.push(format!("line {} at {}, wanted {}: {}", n + 1, indent, wanted * 4, body));
            }
        }
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' if in_string => {
                    chars.next();
                }
                '"' => in_string = !in_string,
                '{' if !in_string => depth += 1,
                '}' if !in_string => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    wrong
}

fn canonical(sandbox: &Sandbox, when: &str) {
    let week = on_disk(&sandbox.0, WEEK);
    let wrong = out_of_place(&week);
    assert!(wrong.is_empty(), "{}, the week is not at its depths:\n{}\n\n{}", when, wrong.join("\n"), week);
}

#[test]
fn an_item_finished_into_a_new_week_and_then_another_are_written_at_their_depth() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = a_tracker("new", None);
    // One with a conversation, so that there is something deeper than its fields.
    pick(&documents, "brace", "finished");
    assert!(on_disk(&sandbox.0, WEEK).contains("handle = \"brace\""));
    canonical(&sandbox, "after the first finish");
    pick(&documents, "favicon", "finished");
    assert!(on_disk(&sandbox.0, WEEK).contains("handle = \"favicon\""));
    canonical(&sandbox, "after the second finish");
    pick(&documents, "rows", "finished");
    canonical(&sandbox, "after the third finish");
}

#[test]
fn a_week_written_again_keeps_what_it_holds_at_its_depth() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = a_tracker("again", None);
    pick(&documents, "brace", "finished");
    pick(&documents, "favicon", "finished");
    // Reopening one takes it out of the week, which is written again with the other.
    documents.run_event("p.os", "project/History/[favicon]/reopen", "click").expect("the reopen");
    assert!(!on_disk(&sandbox.0, WEEK).contains("handle = \"favicon\""));
    canonical(&sandbox, "after a reopen");
}

#[test]
fn a_finish_undone_and_made_again_is_written_at_its_depth() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = a_tracker("undo", None);
    pick(&documents, "brace", "finished");
    pick(&documents, "favicon", "finished");
    documents.undo("p.os").expect("the undo");
    canonical(&sandbox, "after an undo");
    pick(&documents, "favicon", "finished");
    assert!(on_disk(&sandbox.0, WEEK).contains("handle = \"favicon\""));
    canonical(&sandbox, "after finishing it again");
}

#[test]
fn a_week_holding_an_entry_out_of_place_is_written_back_at_its_depths() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let misplaced = "list History {\n        - {\n            - finished_at = \"2026-10-05T10:00:00Z\"\n            - title = \"Old one\"\n            - handle = \"old\"\n            div Talk {\n                list Comments {\n                    - {\n                        - author = \"owner\"\n                        - body = \"said\"\n                    }\n                }\n            }\n        }\n}\n";
    let (sandbox, documents) = a_tracker("misplaced", Some(misplaced));
    pick(&documents, "brace", "finished");
    let week = on_disk(&sandbox.0, WEEK);
    assert!(week.contains("handle = \"old\"") && week.contains("handle = \"brace\""), "{}", week);
    canonical(&sandbox, "after a finish into a week holding an entry out of place");
}
