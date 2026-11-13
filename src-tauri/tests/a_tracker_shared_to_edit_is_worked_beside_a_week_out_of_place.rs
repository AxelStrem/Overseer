//! A tracker shared with someone to edit is theirs to work - claim an item, move it a stage -
//! whatever its History's weeks look like.
//!
//! A week holding an entry out of place is written back at its depths the next time something is
//! written into it (see `an_entry_finished_into_a_part_is_written_at_its_depth`). A part is its
//! owner's to write, so a write that reaches no entry of the week must not take the chance to
//! tidy it: for whoever the tracker is only shared with, that turned every write into a refusal.

use overseer::access::Asker;
use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;
use std::path::PathBuf;

const TRACKER: &str = "<projects/project_template.os/project> project (label=\"P\") {\n    - name = \"P\"\n    mount people (source=\"../../people.os/People\", hidden=true, lazy=false, mutable=false) { }\n    - Items {\n        - {\n            - added = \"2026-10-01T10:00:00+00:00\"\n            - handle = \"brace\"\n            - owner = \"ann\"\n            - title = \"A brace\"\n            - points = 2\n        }\n    }\n    - History {\n    }\n}\n";
const PEOPLE: &str = "div (hidden=true) {\n    div Person (layout=\"horizontal\") {\n        string tag = \"\"\n        string name = \"\"\n    }\n}\n\nlist People (entry=<Person>, key=\"tag\") {\n    - {\n        - tag = \"ann\"\n        - name = \"Ann\"\n    }\n    - {\n        - tag = \"bob\"\n        - name = \"Bob\"\n    }\n}\n";
const WEEK: &str = "ann/projects/p.parts/History/2026-W41.os";
const MISPLACED: &str = "list History {\n        - {\n            - finished_at = \"2026-10-05T10:00:00Z\"\n            - title = \"Old one\"\n            - handle = \"old\"\n            div Talk {\n                list Comments {\n                    - {\n                        - author = \"owner\"\n                        - body = \"said\"\n                    }\n                }\n            }\n        }\n}\n";
const SHARING: &str = "div (hidden=true) {\n    div Share {\n        string document = \"\"\n        string with = \"\"\n        string can = \"read\"\n    }\n}\n\nlist Shares (entry=<Share>) {\n    - {\n        - document = \"projects/p.os\"\n        - with = \"bob\"\n        - can = \"edit\"\n    }\n}\n";

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        FormulaEvaluator::set_time_override(None);
        overseer::document_disk::set_override(None);
    }
}

fn anns_tracker(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_sharedweek_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("ann/projects/p.parts/History")).unwrap();
    std::fs::write(root.join("ann/projects/p.os"), TRACKER).unwrap();
    std::fs::write(root.join("people.os"), PEOPLE).unwrap();
    std::fs::write(root.join("ann/sharing.os"), SHARING).unwrap();
    std::fs::write(root.join(WEEK), MISPLACED).unwrap();
    overseer::document_disk::set_override(Some(None));
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-06T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    let documents = DocumentRoot::new(&root).unwrap().with_accounts(|name| name == "ann" || name == "bob");
    (Sandbox(root), documents)
}

fn bob() -> Asker {
    Asker::Person("bob".to_string())
}

#[test]
fn a_field_set_by_whoever_it_is_shared_with_is_written() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = anns_tracker("field");
    let set = documents.as_asker(bob(), || {
        documents.set_at("ann/projects/p.os", "project/Items/[brace]/title", OverseerValue::String("Renamed".into()))
    });
    assert!(set.is_ok(), "{:?}", set.err());
    assert!(std::fs::read_to_string(sandbox.0.join("ann/projects/p.os")).unwrap().contains("Renamed"));
    assert_eq!(std::fs::read_to_string(sandbox.0.join(WEEK)).unwrap(), MISPLACED);
}

#[test]
fn a_stage_moved_by_whoever_it_is_shared_with_is_written() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = anns_tracker("stage");
    let set = documents.as_asker(bob(), || {
        documents.set_at("ann/projects/p.os", "project/Items/[brace]/stage", OverseerValue::String("testing".into()))
    });
    assert!(set.is_ok(), "{:?}", set.err());
    assert_eq!(std::fs::read_to_string(sandbox.0.join(WEEK)).unwrap(), MISPLACED);
}

#[test]
fn a_claim_by_whoever_it_is_shared_with_is_written() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = anns_tracker("claim");
    let pressed = documents.as_asker(bob(), || documents.run_event("ann/projects/p.os", "project/Items/[brace]/claim", "click"));
    assert!(pressed.is_ok(), "{:?}", pressed.err());
    assert_eq!(std::fs::read_to_string(sandbox.0.join(WEEK)).unwrap(), MISPLACED);
}
