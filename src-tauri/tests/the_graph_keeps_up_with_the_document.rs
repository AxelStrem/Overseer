//! The dependency graph describes the document as it is now, not as it was opened.
//!
//! It was recorded once, when a document was opened, and carried along unchanged. Two things
//! went wrong with that, both reproduced on 2026-09-27.
//!
//! A change of shape - an entry added or taken out - works the whole document out again, and the
//! cache then moved the old graph onto the new text. So an entry added since was unknown to it:
//! a meal added with 100 g and corrected to 300 kept its 150 calories, and the day its old total,
//! until the document was opened again. The document is recorded while it is worked out whole now.
//!
//! And a formula reads what its branches lead it to: `flag ? b : c` read `b` when it was opened,
//! so once `flag` was off, an edit to `c` reached nothing. What an edit works out again is recorded
//! and added to the graph now - added to, never put in place of, because a value reached but
//! settled without reading again would come back with fewer sources than it has.

use overseer::app_api;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Meal (layout="horizontal") {
            float grams = 0
            float calories = $(grams * 1.5)
        }
    }

    list intake (entry=<Meal>) {
        - {
            - grams = 200
        }
    }
    float total = $(intake.map(|x| x/calories).sum())

    button add (label="add") {
        on click {
            append (list="/t/intake") {
                - grams = 100
            }
        }
    }

    bool flag = true
    int b = 1
    int c = 2
    int a = $(flag ? b : c)
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

/// A file holding the document, and the document as the page holds it once opened.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_graph_keeps_up_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

/// Take an answer on, the way the page does.
fn take(page: &mut Vec<OverseerNode>, answer: app_api::ResolvedUpdate) {
    if let Some(whole) = answer.nodes {
        *page = whole;
        return;
    }
    overseer::delta::apply(page, answer.changes.expect("an answer describes something"));
}

fn number(page: &[OverseerNode], address: &str) -> Option<f64> {
    match overseer::addressing::find(page, address)?.parameters.get("_computed_value") {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

/// The name path of the last meal's grams, as the page would name it.
fn last_meal_grams(page: &[OverseerNode]) -> Vec<String> {
    let address = overseer::addressing::addresses(page)
        .into_iter()
        .filter(|a| a.starts_with("t/intake/") && a.ends_with("/grams"))
        .last()
        .expect("a meal");
    overseer::addressing::name_path(page, &address).expect("its path")
}

fn write(path: &str, page: &mut Vec<OverseerNode>, field: impl FnOnce(&[OverseerNode]) -> Vec<String>, value: OverseerValue) {
    let field = field(page);
    let w: app_api::ValueWrite =
        serde_json::from_value(serde_json::json!({ "node_path": field, "value": value })).unwrap();
    let answer = app_api::write_values_at(path, "d.os", vec![w], "s").expect("the write");
    take(page, answer);
}

#[test]
fn a_meal_added_and_then_corrected_is_counted_as_corrected() {
    serialised(|| {
        let (path, mut page) = opened("append");
        let fields = [("grams".to_string(), OverseerValue::Float(100.0))].into_iter().collect();
        let added = app_api::append_entry_at(&path, "d.os", vec!["t".into(), "intake".into()], fields, "s")
            .expect("the append");
        take(&mut page, added);
        assert_eq!(number(&page, "t/total"), Some(450.0));

        let new_meal = overseer::addressing::addresses(&page)
            .into_iter()
            .filter(|a| a.starts_with("t/intake/") && a.ends_with("/grams"))
            .last()
            .unwrap();
        write(&path, &mut page, last_meal_grams, OverseerValue::Float(300.0));
        let calories = new_meal.replace("/grams", "/calories");
        assert_eq!(number(&page, &calories), Some(450.0), "the new meal kept its old calories");
        assert_eq!(number(&page, "t/total"), Some(750.0), "the day kept its old total");
    });
}

#[test]
fn a_meal_added_by_a_press_and_then_corrected() {
    serialised(|| {
        let (path, mut page) = opened("press");
        let button = overseer::addressing::name_path(&page, "t/add").expect("the button");
        let pressed = app_api::run_event_at(&path, "d.os", button, "click".into(), "s", Vec::new(), Vec::new())
            .expect("the press");
        take(&mut page, pressed);
        assert_eq!(number(&page, "t/total"), Some(450.0));
        write(&path, &mut page, last_meal_grams, OverseerValue::Float(300.0));
        assert_eq!(number(&page, "t/total"), Some(750.0));
    });
}

#[test]
fn what_is_left_after_a_removal_is_still_followed() {
    serialised(|| {
        let (path, mut page) = opened("remove");
        let fields = [("grams".to_string(), OverseerValue::Float(100.0))].into_iter().collect();
        take(&mut page, app_api::append_entry_at(&path, "d.os", vec!["t".into(), "intake".into()], fields, "s").unwrap());
        let first = overseer::addressing::addresses(&page)
            .into_iter()
            .find(|a| a.starts_with("t/intake/") && a.matches('/').count() == 2)
            .unwrap();
        let first = overseer::addressing::name_path(&page, &first).unwrap();
        take(&mut page, app_api::remove_entry_at(&path, "d.os", first, "s").expect("the removal"));
        assert_eq!(number(&page, "t/total"), Some(150.0));
        write(&path, &mut page, last_meal_grams, OverseerValue::Float(300.0));
        assert_eq!(number(&page, "t/total"), Some(450.0));
    });
}

#[test]
fn a_branch_taken_later_is_followed() {
    serialised(|| {
        let (path, mut page) = opened("branch");
        let field = |a: &'static str| move |page: &[OverseerNode]| overseer::addressing::name_path(page, a).unwrap();
        write(&path, &mut page, field("t/flag"), OverseerValue::Boolean(false));
        assert_eq!(number(&page, "t/a"), Some(2.0));
        write(&path, &mut page, field("t/c"), OverseerValue::Integer(7));
        assert_eq!(number(&page, "t/a"), Some(7.0), "a no longer follows c");
        // And the branch it left is still followed too: a union never forgets.
        write(&path, &mut page, field("t/flag"), OverseerValue::Boolean(true));
        write(&path, &mut page, field("t/b"), OverseerValue::Integer(5));
        assert_eq!(number(&page, "t/a"), Some(5.0));
    });
}

#[test]
fn the_same_for_a_document_without_a_file() {
    // The page's other path, for a document it holds only as text.
    serialised(|| {
        app_api::forget_baseline();
        app_api::forget_dependencies();
        let mut page = app_api::load_document(DOCUMENT.to_string()).expect("open");
        let button = overseer::addressing::name_path(&page, "t/add").expect("the button");
        let pressed = app_api::execute_event_update(DOCUMENT.to_string(), button, "click".into()).expect("the press");
        let text = pressed.text.clone();
        take(&mut page, pressed);
        let grams = last_meal_grams(&page).join("/");
        let values = [(grams.clone(), OverseerValue::Float(300.0))].into_iter().collect();
        let edited = app_api::resolve_selective_update(text, vec![grams], Some(values)).expect("the edit");
        take(&mut page, edited);
        assert_eq!(number(&page, "t/total"), Some(750.0));
    });
}
