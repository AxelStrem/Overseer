//! An agent loop takes an item of a project by pressing its hidden `claim`, and another loop
//! pressing it afterwards is refused.
//!
//! Loops run side by side on one tracker, and two of them must never work the same item. Reading
//! `working` and then setting it would let two loops both see it free; a press is one write, so the
//! check and the set happen together and only the first press gets the item.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;

const EXAMPLE: &str = include_str!("../../examples/projects/project.os");

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

/// The example project in a file of its own.
fn written(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_project_claim_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, EXAMPLE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    file.to_string_lossy().to_string()
}

fn press_claim(file: &str, handle: &str) -> overseer::types::Result<app_api::ResolvedUpdate> {
    let nodes = app_api::load_document(std::fs::read_to_string(file).unwrap()).expect("open");
    let button = overseer::addressing::name_path(&nodes, &format!("project/Items/[{}]/claim", handle))
        .expect("the claim button");
    app_api::run_event_at(file, "p.os", button, "click".into(), "s", Vec::new(), Vec::new())
}

#[test]
fn the_first_claim_takes_the_item_and_the_second_is_refused() {
    serialised(|| {
        let file = written("twice");
        assert!(!EXAMPLE.contains("- working = true"));

        let first = press_claim(&file, "brace").expect("the first claim");
        assert!(first.wrote, "claiming wrote nothing");
        let claimed = std::fs::read_to_string(&file).unwrap();
        assert!(claimed.contains("- working = true"), "{}", claimed);

        let second = press_claim(&file, "brace");
        let refusal = format!("{:?}", second.err().expect("a second claim went through"));
        assert!(refusal.contains("another agent is working on it"), "{}", refusal);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), claimed, "the refused claim wrote something");
    });
}

#[test]
fn claiming_one_item_leaves_the_others_free() {
    serialised(|| {
        let file = written("others");
        press_claim(&file, "brace").expect("the claim");
        let other = press_claim(&file, "editor").expect("another item's claim");
        assert!(other.wrote);
        assert_eq!(std::fs::read_to_string(&file).unwrap().matches("- working = true").count(), 2);
    });
}
