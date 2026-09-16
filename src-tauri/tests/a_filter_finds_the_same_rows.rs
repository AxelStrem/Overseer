//! A filter answered from an index returns exactly the rows the walk would have returned.
//!
//! Looking a food up by its handle is the single most repeated question these documents ask - the
//! food tracker asks it about fourteen nutrients for every entry on every pass - and it used to be
//! answered by walking the whole catalogue each time. It is now answered from an index built once
//! per list per pass.
//!
//! An index may only stand in for the walk where the two cannot disagree, and equality here is not
//! as simple as it looks: `compare_values` falls back to comparing the two sides written out as
//! text, so `5` equals `"5"` while `5.0` does not equal `"5.0"`. Equality that is not transitive
//! cannot be grouped into buckets, so anything but strings on both sides falls back to the walk.
//! These are the cases where getting that wrong would show.

use overseer::app_api;
use overseer::types::{OverseerNode, OverseerValue};

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(f) = find(&n.children, name) {
            return Some(f);
        }
    }
    None
}

/// What a named field worked out to.
fn value_of(source: &str, field: &str) -> OverseerValue {
    let nodes = app_api::load_document(source.to_string()).expect("document did not load");
    let node = find(&nodes, field).unwrap_or_else(|| panic!("no field named `{field}`"));
    node.parameters
        .get("_computed_value")
        .cloned()
        .unwrap_or(OverseerValue::Null)
}

fn number(source: &str, field: &str) -> f64 {
    match value_of(source, field) {
        OverseerValue::Integer(i) => i as f64,
        OverseerValue::Float(f) => f,
        other => panic!("`{field}` is not a number: {other:?}"),
    }
}

fn text(source: &str, field: &str) -> String {
    match value_of(source, field) {
        OverseerValue::String(s) => s,
        other => panic!("`{field}` is not a string: {other:?}"),
    }
}

/// The shape the food tracker uses: a catalogue keyed by a string handle, and fields that reach
/// into it by that handle.
const CATALOGUE: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Food (layout="horizontal") {
            string handle (label="") = ""
            float per_100g (label="") = 0
        }
    }

    list Catalog (entry=<Food>, key="handle") {
        - {
            - handle = "oats"
            - per_100g = 13
        }
        - {
            - handle = "milk"
            - per_100g = 3
        }
        - {
            - handle = "beef"
            - per_100g = 26
        }
    }

    string wanted (label="") = "milk"
    float looked_up (label="") = $(/t/Catalog.filter(|x| x/handle == wanted)/per_100g)
    float first_one (label="") = $(/t/Catalog.filter(|x| x/handle == "oats")/per_100g)
    float last_one (label="") = $(/t/Catalog.filter(|x| x/handle == "beef")/per_100g)
    float absent (label="") = $(/t/Catalog.filter(|x| x/handle == "kale").map(|x| x/per_100g).sum())
    float not_equal (label="") = $(/t/Catalog.filter(|x| x/handle != "milk").map(|x| x/per_100g).sum())
}
"#;

#[test]
fn a_row_is_found_by_the_string_it_holds() {
    assert_eq!(number(CATALOGUE, "looked_up"), 3.0);
    assert_eq!(number(CATALOGUE, "first_one"), 13.0);
    assert_eq!(number(CATALOGUE, "last_one"), 26.0);
}

#[test]
fn a_handle_nothing_holds_finds_nothing() {
    assert_eq!(number(CATALOGUE, "absent"), 0.0);
}

#[test]
fn an_inequality_is_not_an_index_lookup() {
    // Only equality can be answered from buckets; everything else still walks, and must still be
    // right. All three rows but milk.
    assert_eq!(number(CATALOGUE, "not_equal"), 39.0);
}

const DUPLICATES: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string kind (label="") = ""
            int n (label="") = 0
        }
    }

    list Rows (entry=<Row>) {
        - {
            - kind = "same"
            - n = 1
        }
        - {
            - kind = "other"
            - n = 9
        }
        - {
            - kind = "same"
            - n = 2
        }
        - {
            - kind = "same"
            - n = 3
        }
    }

    int how_many (label="") = $(/t/Rows.filter(|x| x/kind == "same").count())
    int summed (label="") = $(/t/Rows.filter(|x| x/kind == "same").map(|x| x/n).sum())
    int in_order (label="") = $(/t/Rows.filter(|x| x/kind == "same").reduce(0, |acc, x| acc * 10 + x/n))
}
"#;

#[test]
fn every_row_holding_the_value_comes_back() {
    assert_eq!(number(DUPLICATES, "how_many"), 3.0);
    assert_eq!(number(DUPLICATES, "summed"), 6.0);
}

#[test]
fn the_rows_come_back_in_the_order_they_sit_in() {
    // 1, 2, 3 read as digits. A bucket handed back in any other order - or one built by walking
    // the list backwards - would read 321 or 213 here.
    assert_eq!(number(DUPLICATES, "in_order"), 123.0);
}

const TWO_LISTS: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string kind (label="") = ""
            string note (label="") = ""
        }
    }

    list Left (entry=<Row>, key="kind") {
        - {
            - kind = "a"
            - note = "from the left"
        }
    }

    list Right (entry=<Row>, key="kind") {
        - {
            - kind = "a"
            - note = "from the right"
        }
    }

    string left_note (label="") = $(/t/Left.filter(|x| x/kind == "a")/note)
    string right_note (label="") = $(/t/Right.filter(|x| x/kind == "a")/note)
}
"#;

#[test]
fn two_lists_asked_the_same_question_get_their_own_answers() {
    // Both key on `kind` and both hold "a". An index keyed by the field name alone, or shared
    // between lists, would hand the second list the first one's row.
    assert_eq!(text(TWO_LISTS, "left_note"), "from the left");
    assert_eq!(text(TWO_LISTS, "right_note"), "from the right");
}

const NOT_STRINGS: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Row (layout="horizontal") {
            int code (label="") = 0
            string name (label="") = ""
        }
    }

    list Rows (entry=<Row>) {
        - {
            - code = 5
            - name = "five"
        }
        - {
            - code = 7
            - name = "seven"
        }
    }

    string by_number (label="") = $(/t/Rows.filter(|x| x/code == 5)/name)
    string by_numeric_string (label="") = $(/t/Rows.filter(|x| x/code == "5")/name)
}
"#;

#[test]
fn a_field_holding_numbers_is_not_indexed_but_is_still_matched() {
    // The index only groups strings, so this list is refused and walked. Both have to keep
    // working: `compare_values` compares two numbers numerically, and a number against a string
    // by writing both out as text - which is the looseness that cannot be put into buckets.
    assert_eq!(text(NOT_STRINGS, "by_number"), "five");
    assert_eq!(text(NOT_STRINGS, "by_numeric_string"), "five");
}

const CHAINED: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string kind (label="") = ""
            string colour (label="") = ""
            int n (label="") = 0
        }
    }

    list Rows (entry=<Row>) {
        - {
            - kind = "fruit"
            - colour = "red"
            - n = 1
        }
        - {
            - kind = "fruit"
            - colour = "green"
            - n = 2
        }
        - {
            - kind = "veg"
            - colour = "red"
            - n = 4
        }
    }

    int both (label="") = $(/t/Rows.filter(|x| x/kind == "fruit").filter(|x| x/colour == "red").map(|x| x/n).sum())
    int second_only (label="") = $(/t/Rows.filter(|x| x/colour == "red").map(|x| x/n).sum())
}
"#;

#[test]
fn a_second_filter_narrows_what_the_first_left() {
    // Only the first call in a chain may be answered from an index - after that the list is no
    // longer the one the index describes. A second filter reading the index built for the whole
    // list would bring the veg back too, for five.
    assert_eq!(number(CHAINED, "both"), 1.0);
    assert_eq!(number(CHAINED, "second_only"), 5.0);
}

const UNSET: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string kind (label="")
            int n (label="") = 0
        }
    }

    list Rows (entry=<Row>) {
        - {
            - kind = "here"
            - n = 1
        }
        - {
            - n = 2
        }
    }

    int found (label="") = $(/t/Rows.filter(|x| x/kind == "here").map(|x| x/n).sum())
}
"#;

#[test]
fn a_row_with_nothing_in_the_field_fails_the_whole_filter() {
    // Not what anyone would want, and not something the index did: with indexing switched off
    // entirely this document gives the same answer, so it is recorded here rather than fixed
    // under cover of a performance change.
    //
    // The list cannot be indexed - one row holds no `kind` at all - so it is walked. The two
    // walks then disagree with each other: the one that resolves a node skips a row whose
    // predicate will not evaluate, while the one that produces a value propagates the failure
    // with `?` and loses the whole formula. The row that does hold "here" is never reached, and
    // a sum that should be 1 reads as an error instead.
    //
    // Worth fixing on its own, with its own thought about what a predicate that cannot be
    // answered should mean. When it is, this test should say `assert_eq!(number(..), 1.0)`.
    assert_eq!(
        value_of(UNSET, "found"),
        OverseerValue::String("invalid formula error".to_string()),
    );
}
