//! Everything one answer says was worked out at one moment.
//!
//! `now()` read the clock afresh on every call, so a minute could turn over halfway through working
//! a document out and two values in one answer disagreed about when it was - a rule's minutes since
//! it was last done read one number and the task it opened another, and a test comparing two
//! resolves failed whenever a minute happened to turn over between them. The clock is read once at
//! the start of each piece of work now - a resolve, an open, a press, a write - and held until it
//! is done.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    timestamp first = $(now())
    timestamp second = $(now())
    timestamp stamped = ""
    // Reads the stamp, so a press that stamps works this out as well - and reads the clock.
    timestamp seen = $(stamped != "" ? now() : now())

    button stamp (label="stamp") {
        on click {
            set (path="../stamped") = $(now())
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

fn shown(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let node = overseer::addressing::find(nodes, address)?;
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

#[test]
fn two_readings_in_one_resolve_are_one_reading() {
    serialised(|| {
        let (_rest, mut nodes) = overseer::parser::parse_document(DOCUMENT).expect("parse");
        overseer::resolver::resolve_document(&mut nodes);
        let first = shown(&nodes, "t/first");
        assert!(matches!(first, Some(OverseerValue::Timestamp(_))), "{:?}", first);
        assert_eq!(first, shown(&nodes, "t/second"), "the clock moved during one resolve");
    });
}

#[test]
fn the_clock_is_let_go_afterwards() {
    serialised(|| {
        let (_rest, mut nodes) = overseer::parser::parse_document(DOCUMENT).expect("parse");
        overseer::resolver::resolve_document(&mut nodes);
        assert_eq!(FormulaEvaluator::get_time_override(), None, "the clock stayed pinned");
    });
}

#[test]
fn a_clock_set_by_hand_is_left_as_it_was() {
    serialised(|| {
        let when = chrono::DateTime::parse_from_rfc3339("2026-09-27T09:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        FormulaEvaluator::set_time_override(Some(when));
        let (_rest, mut nodes) = overseer::parser::parse_document(DOCUMENT).expect("parse");
        overseer::resolver::resolve_document(&mut nodes);
        let held = FormulaEvaluator::get_time_override();
        FormulaEvaluator::set_time_override(None);
        assert_eq!(held, Some(when), "a pin let go of a clock it had not set");
        match shown(&nodes, "t/first") {
            Some(OverseerValue::Timestamp(s)) => assert!(s.starts_with("2026-09-27T09:00:00"), "{}", s),
            other => panic!("{:?}", other),
        }
    });
}

#[test]
fn a_press_and_what_it_works_out_agree_about_when() {
    // The moment an action stamps is the moment everything the press works out reads. `first`
    // is no witness: nothing it reads moves, so the press rightly leaves it as the open had it.
    serialised(|| {
        let root = std::env::temp_dir().join(format!("overseer_clock_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("c.os");
        std::fs::write(&file, DOCUMENT).unwrap();
        app_api::forget_baseline();
        app_api::forget_dependencies();
        let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
        let button = overseer::addressing::name_path(&nodes, "t/stamp").unwrap();
        let answer = app_api::run_event_at(&file.to_string_lossy(), "c.os", button, "click".into(), "s", Vec::new())
            .expect("the press");
        // Taken on the way the page takes it - the answer, not a later open, which reads the
        // clock at its own moment.
        let mut now = nodes;
        match (answer.changes, answer.nodes) {
            (_, Some(whole)) => now = whole,
            (Some(changes), None) => {
                for change in changes {
                    if let overseer::delta::DocumentChange::Parameters { path, parameters, .. } = change {
                        let mut node = &mut now[path[0]];
                        for i in &path[1..] {
                            node = &mut node.children[*i];
                        }
                        node.parameters = parameters;
                    }
                }
            }
            (None, None) => panic!("the answer described nothing"),
        }
        // The moments themselves, to the microsecond a stamp is kept to.
        let moment = |v: Option<OverseerValue>| match v {
            Some(OverseerValue::Timestamp(s)) => chrono::DateTime::parse_from_rfc3339(&s)
                .map(|t| t.timestamp_micros())
                .unwrap_or_else(|e| panic!("{}: {}", s, e)),
            other => panic!("not a moment: {:?}", other),
        };
        assert_eq!(
            moment(shown(&now, "t/stamped")),
            moment(shown(&now, "t/seen")),
            "the press stamped one moment and worked out another"
        );
    });
}
