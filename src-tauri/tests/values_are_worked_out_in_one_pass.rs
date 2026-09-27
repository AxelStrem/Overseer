//! A document is worked out in one pass over its formulas.
//!
//! It used to take as many passes as its longest chain had links, because every read in a pass
//! came from what the pass before had stored: a meal's calories, the day's total, the share of it
//! that was vegan, the grade - one link a pass, and a pass is the whole document. The food tracker
//! took six, the project tracker six at nearly a second each, and every document at least two,
//! the last only to find that nothing had moved.
//!
//! Now a read of something the pass has not stored yet works it out where it lives, and gets the
//! answer the pass will store for it. What took the passes, and is pinned here:
//!
//! - a list's entries read by a lambda or an aggregate were read as nothing, or worked out against
//!   the path of whatever was asking, where `../food` names something else;
//! - a formula that fails read as an error that failed its reader too, where the stored failure is
//!   text a comparison simply finds false;
//! - an unset field with a literal default read as the default before its fallback was tried;
//! - a `tags` field worked out from elsewhere read as no tags at all;
//! - a value left behind by an earlier resolve was taken as though it were current.
//!
//! A circle - `grams` falling back to `portions` and back, where neither is stated - is cut, and
//! the few nodes that met the cut are worked out once more from what the pass stored, rather than
//! the whole document.
//!
//! An edit is worked out the same way over what it reaches. It had four passes, and a chain of five
//! links from a meal's grams to the day's grade stopped one short: the grade stayed what it was.

use overseer::app_api;
use overseer::delta::DocumentChange;
use overseer::parser;
use overseer::resolver;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    div (hidden=true) {
        div Food (layout="horizontal") {
            string handle = ""
            float per_gram = 0
            float portion = 0
            string kind = ""
        }

        // A meal states its grams or its portions, and the other is worked out from it. This
        // declaration states neither, so here each falls back to the other: a circle.
        div Meal (layout="horizontal") {
            string food = ""
            float portion_weight = $(Foods.filter(|x| x/handle == ../food)/portion)
            float per_gram = $(Foods.filter(|x| x/handle == ../food)/per_gram)
            float portions (fallback=$(grams / portion_weight), default=1) = null
            float grams (fallback=$(portions * portion_weight)) = null
            float calories = $(grams * per_gram)
            tags labels = $(Foods.filter(|x| x/handle == ../food)/kind)
        }
    }

    list Foods (entry=<Food>, key="handle") {
        - {
            - handle = "tofu"
            - per_gram = 1.5
            - portion = 100
            - kind = "vegan"
        }
        - {
            - handle = "steak"
            - per_gram = 2.5
            - portion = 200
            - kind = "meat"
        }
    }

    list intake (entry=<Meal>) {
        - {
            - food = "tofu"
            - grams = 200
        }
        - {
            - food = "steak"
            - portions = 1
        }
    }

    float total = $(intake.map(|x| x/calories).sum())
    float vegan = $(intake.filter(|x| x/labels.filter(|tag| tag == "vegan").count() > 0).map(|x| x/calories).sum())
    float share = $(total > 0 ? vegan / total * 100 : 0)
    int grade = $(share > 30 ? 1 : 0)

    // Nothing is called `missing`, so this fails - and what reads it reads the failure.
    float broken = $(missing * 2)
    int after_broken = $(broken > 10 ? 1 : 0)
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn resolved(text: &str) -> Vec<OverseerNode> {
    let (_rest, mut nodes) = parser::parse_document(text).expect("parse");
    resolver::resolve_document(&mut nodes);
    nodes
}

fn at<'a>(nodes: &'a [OverseerNode], address: &str) -> &'a OverseerNode {
    overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("nothing at {}", address))
}

fn number(v: Option<&OverseerValue>) -> Option<f64> {
    match v {
        Some(OverseerValue::Integer(i)) => Some(*i as f64),
        Some(OverseerValue::Float(f)) => Some(*f),
        _ => None,
    }
}

fn value(nodes: &[OverseerNode], address: &str) -> Option<f64> {
    number(at(nodes, address).parameters.get("_computed_value"))
}

#[test]
fn a_chain_through_the_entries_of_a_list_is_done_in_one_pass() {
    serialised(|| {
        let nodes = resolved(DOCUMENT);
        assert_eq!(value(&nodes, "t/total"), Some(800.0), "300 of tofu and 500 of steak");
        assert_eq!(value(&nodes, "t/vegan"), Some(300.0), "the tofu, by its tags");
        assert_eq!(value(&nodes, "t/share"), Some(37.5));
        assert_eq!(value(&nodes, "t/grade"), Some(1.0));
        assert_eq!(resolver::passes_taken(), 1, "the chain took more than one pass");
    });
}

#[test]
fn a_failure_reads_as_the_failure_it_is_stored_as() {
    serialised(|| {
        let nodes = resolved(DOCUMENT);
        assert_eq!(
            at(&nodes, "t/broken").parameters.get("_computed_value"),
            Some(&OverseerValue::String("invalid formula error".into()))
        );
        assert_eq!(resolver::passes_taken(), 1);
        // What reads it came out the same in that one pass as it does worked out again from the
        // failure the pass stored - which is the whole of what a second pass would have done.
        let in_the_pass = value(&nodes, "t/after_broken");
        let mut again = nodes.clone();
        resolver::resolve_specific_fields(&mut again, &["t/after_broken".to_string()].into_iter().collect());
        assert_eq!(value(&again, "t/after_broken"), in_the_pass, "read on demand and read back disagree");
    });
}

#[test]
fn a_fallback_is_tried_before_a_default() {
    serialised(|| {
        let nodes = resolved(DOCUMENT);
        // The steak states one portion; its grams come from that, 200, and its calories from those.
        let steak_calories = overseer::addressing::addresses(&nodes)
            .into_iter()
            .filter(|a| a.starts_with("t/intake/") && a.ends_with("/calories"))
            .map(|a| value(&nodes, &a))
            .collect::<Vec<_>>();
        assert!(steak_calories.contains(&Some(500.0)), "{:?}", steak_calories);
    });
}

#[test]
fn nothing_left_behind_by_an_earlier_resolve_is_taken_as_current() {
    serialised(|| {
        let mut nodes = resolved(DOCUMENT);
        // Four hundred grams of tofu now, written into the tree the resolve left - with every
        // value it worked out still on it.
        let tofu_grams = overseer::addressing::addresses(&nodes)
            .into_iter()
            .find(|a| a.starts_with("t/intake/") && a.ends_with("/grams"))
            .unwrap();
        let path = overseer::addressing::name_path(&nodes, &tofu_grams).unwrap();
        overseer::actions::ActionExecutor::assign_value(&mut nodes, &format!("/{}", path.join("/")), OverseerValue::Float(400.0))
            .expect("the write");
        resolver::resolve_document(&mut nodes);
        assert_eq!(value(&nodes, "t/total"), Some(1100.0));
        assert_eq!(value(&nodes, "t/vegan"), Some(600.0));
        assert_eq!(resolver::passes_taken(), 1);
    });
}

#[test]
fn an_edit_reaches_the_end_of_a_long_chain() {
    // grams, calories, the total and the vegan share of it, the share, the grade: five links, one
    // more than an edit's passes used to allow.
    serialised(|| {
        let root = std::env::temp_dir().join(format!("overseer_one_pass_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("d.os");
        std::fs::write(&file, DOCUMENT).unwrap();
        app_api::forget_baseline();
        overseer::viewstate::forget_all();
        let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
        assert_eq!(value(&nodes, "t/grade"), Some(1.0));

        let tofu_grams = overseer::addressing::addresses(&nodes)
            .into_iter()
            .find(|a| a.starts_with("t/intake/") && a.ends_with("/grams"))
            .unwrap();
        let field = overseer::addressing::name_path(&nodes, &tofu_grams).unwrap();
        let w: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
            "node_path": field, "value": OverseerValue::Float(20.0)
        }))
        .unwrap();
        let answer = app_api::write_values_at(&file.to_string_lossy(), "d.os", vec![w], "s").expect("the write");
        assert_eq!(resolver::passes_taken(), 1, "the edit took more than one pass");
        let grade = answer.changes.expect("answered with what changed").into_iter().find_map(|c| match c {
            DocumentChange::Parameters { address, parameters, .. } if address == "t/grade" => {
                number(parameters.get("_computed_value"))
            }
            _ => None,
        });
        assert_eq!(grade, Some(0.0), "thirty calories of tofu in 530 is not a third");
    });
}
