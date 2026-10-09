//! `fold`, `unfold` and `toggle_fold` fold a div away and bring it back.
//!
//! What is folded is how somebody is looking at the document rather than anything it says, so it
//! is the page's to keep - like opening a field for editing. The actions find the div by the same
//! rules as every other action's path, and the answer names it, in order, by the child indices the
//! page finds it with. Nothing is written and nothing is worked out again.

use overseer::actions::FoldHow;
use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    // Read on every resolve, so whether the document was worked out again shows.
    timestamp checked (hidden=true) = $(now())

    bool fold_all (mutable="guarded") = false

    div (hidden=true) {
        div Task (layout="vertical") {
            string key (hidden=true) = ""
            string title = ""
            div Notes (foldable=true, folded=$(/t/fold_all), label="Notes") {
                string comment = ""
            }
            div (layout="horizontal") {
                button hide (label="hide") {
                    on click {
                        fold (path="../Notes")
                    }
                }
                button flip (label="flip") {
                    on click {
                        unfold (path="../Notes")
                        toggle_fold (path="../Notes")
                    }
                }
                button wrong (label="wrong") {
                    on click {
                        fold (path="../title")
                    }
                }
            }
        }
    }

    list Open (entry=<Task>, key="key") {
        - {
            - key = "a"
            - title = "first"
        }
        - {
            - key = "b"
            - title = "second"
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

fn at(when: &str) {
    let instant = chrono::DateTime::parse_from_rfc3339(when)
        .expect("bad instant")
        .with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
}

/// A file holding the document, opened the way the page opens it, at nine o'clock.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_fold_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    at("2026-09-26T09:00:00Z");
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

fn press(path: &str, nodes: &[OverseerNode], button: &str) -> overseer::Result<app_api::ResolvedUpdate> {
    let node_path = overseer::addressing::name_path(nodes, button).expect("the button");
    app_api::run_event_at(path, "d.os", node_path, "click".into(), "s", Vec::new(), Vec::new())
}

fn node_at<'a>(nodes: &'a [OverseerNode], path: &[usize]) -> &'a OverseerNode {
    let mut node = &nodes[path[0]];
    for i in &path[1..] {
        node = &node.children[*i];
    }
    node
}

#[test]
fn the_answer_names_the_div_to_fold() {
    serialised(|| {
        let (path, nodes) = opened("names");
        at("2026-09-26T09:05:00Z");
        let answer = press(&path, &nodes, "t/Open/[b]/hide").expect("the press was refused");
        FormulaEvaluator::set_time_override(None);

        assert_eq!(answer.folds.len(), 1, "{:?}", answer.folds);
        let fold = &answer.folds[0];
        assert_eq!(fold.address, "t/Open/[b]/Notes");
        assert_eq!(fold.how, FoldHow::Fold);
        assert_eq!(node_at(&nodes, &fold.path).name, "Notes", "the path reaches something else");
        assert!(!answer.wrote, "folding wrote the file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    });
}

#[test]
fn a_press_that_only_folds_works_nothing_out() {
    // Five minutes after the document was opened, a resolve would move `checked` - so an answer
    // that says nothing changed is one that was not worked out again.
    serialised(|| {
        let (path, nodes) = opened("nothing");
        at("2026-09-26T09:05:00Z");
        let answer = press(&path, &nodes, "t/Open/[a]/hide").expect("the press was refused");
        FormulaEvaluator::set_time_override(None);
        let changes = answer.changes.expect("answered with what changed");
        assert!(
            changes.is_empty(),
            "the document was worked out again: {:?}",
            changes.iter().map(|c| c.address().to_string()).collect::<Vec<_>>()
        );
    });
}

#[test]
fn the_folds_come_in_the_order_they_were_asked() {
    serialised(|| {
        let (path, nodes) = opened("order");
        let answer = press(&path, &nodes, "t/Open/[a]/flip").expect("the press was refused");
        FormulaEvaluator::set_time_override(None);
        let hows: Vec<FoldHow> = answer.folds.iter().map(|f| f.how).collect();
        assert_eq!(hows, vec![FoldHow::Unfold, FoldHow::Toggle]);
    });
}

#[test]
fn what_cannot_fold_is_refused() {
    serialised(|| {
        let (path, nodes) = opened("wrong");
        let refused = press(&path, &nodes, "t/Open/[a]/wrong");
        FormulaEvaluator::set_time_override(None);
        assert!(refused.is_err(), "folding a field was accepted");
    });
}

#[test]
fn where_a_div_starts_can_be_worked_out() {
    // A "fold all" is a guarded flag the divs read, so `folded` has to be worked out like any
    // other parameter for the page to follow it.
    serialised(|| {
        let (_path, nodes) = opened("formula");
        FormulaEvaluator::set_time_override(None);
        let at = overseer::delta::indices_of(&nodes, "t/Open/[a]/Notes").expect("the div");
        let notes = node_at(&nodes, &at);
        assert_eq!(notes.parameters.get("_computed_folded"), Some(&OverseerValue::Boolean(false)));
    });
}
