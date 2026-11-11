//! A `-` before a value negates it: `-day_of_month(d)`, `-(a + b)`, `-Items.count()`. Only a number
//! literal could carry a sign, so a formula such as the calendar's `previous` button, which steps
//! to the end of the month before with `date_add_days(../showing, -day_of_month(../showing))`,
//! did not parse and its press wrote nothing.

use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::PathBuf;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    date showing = "2026-11-12"
    int a = 4
    int b = 3
    float half = 2.5
    string word = "five"
    int minus_day = $(-day_of_month(showing))
    int minus_a = $(-a)
    int minus_sum = $(-(a + b))
    int minus_minus = $(2 - -3)
    int double_minus = $(--a)
    int tight = $(2--3)
    int spaced = $(a - b)
    int minus_count = $(-Items.count())
    int product = $(-a * b)
    float minus_half = $(-half)
    float minus_literal = $(-2.5)
    bool not_minus = $(!(-a < 0))
    int minus_word = $(-word)

    list Items (entry=<Item>) {
        - { }
        - { }
    }

    button previous (label="Previous") {
        on click {
            set (path="../showing") = $(date_add_days(../showing, -day_of_month(../showing)))
        }
    }

    div (hidden=true) {
        div Item {
            int n = 0
        }
    }
}
"#;

fn folder(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_negation_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), DOCUMENT).unwrap();
    root
}

fn read(root: &PathBuf, address: &str) -> OverseerNode {
    DocumentRoot::new(root).unwrap().read_at("d.os", address).unwrap().node
}

fn value(root: &PathBuf, address: &str) -> OverseerValue {
    let node = read(root, address);
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or(OverseerValue::Null)
}

fn number(root: &PathBuf, address: &str) -> f64 {
    match value(root, address) {
        OverseerValue::Integer(i) => i as f64,
        OverseerValue::Float(f) => f,
        other => panic!("{address} is {other:?}, not a number"),
    }
}

#[test]
fn a_minus_before_a_value_negates_it() {
    let root = folder("values");
    assert_eq!(number(&root, "t/minus_day"), -12.0);
    assert_eq!(number(&root, "t/minus_a"), -4.0);
    assert_eq!(number(&root, "t/minus_sum"), -7.0);
    assert_eq!(number(&root, "t/minus_count"), -2.0);
    assert_eq!(number(&root, "t/minus_half"), -2.5);
    assert_eq!(number(&root, "t/minus_literal"), -2.5);
}

#[test]
fn a_minus_binds_to_the_value_next_to_it() {
    let root = folder("binding");
    assert_eq!(number(&root, "t/product"), -12.0);
    assert_eq!(number(&root, "t/double_minus"), 4.0);
    assert_eq!(value(&root, "t/not_minus"), OverseerValue::Boolean(false));
}

#[test]
fn subtraction_keeps_its_meaning() {
    let root = folder("subtraction");
    assert_eq!(number(&root, "t/minus_minus"), 5.0);
    assert_eq!(number(&root, "t/tight"), 5.0);
    assert_eq!(number(&root, "t/spaced"), 1.0);
}

#[test]
fn text_that_is_not_a_number_cannot_be_negated() {
    let root = folder("text");
    assert_eq!(
        value(&root, "t/minus_word"),
        OverseerValue::String("invalid formula error".to_string())
    );
}

#[test]
fn a_button_steps_a_date_back_by_the_day_of_the_month() {
    let root = folder("button");
    let path = root.join("d.os").to_string_lossy().to_string();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    let node_path = overseer::addressing::name_path(&nodes, "t/previous").expect("the button");
    for expected in ["2026-10-31", "2026-09-30", "2026-08-31"] {
        let nodes = app_api::load_document(std::fs::read_to_string(&path).unwrap()).expect("open");
        let node_path = overseer::addressing::name_path(&nodes, "t/previous").unwrap_or_else(|| node_path.clone());
        app_api::run_event_at(&path, "d.os", node_path, "click".into(), "s", Vec::new(), Vec::new())
            .expect("the press was refused");
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains(&format!("showing = \"{expected}\"")), "after the press:\n{written}");
    }
}
