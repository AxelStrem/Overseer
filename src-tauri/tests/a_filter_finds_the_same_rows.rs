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
fn a_row_with_nothing_in_the_field_does_not_break_the_others() {
    // One row has no `kind`, so the list cannot be indexed and is walked. The row that does hold
    // "here" still has to be found.
    //
    // This used to read "invalid formula error": the walk that produces a value propagated the
    // one row's failure and lost the whole sum, while the walk that resolves a node skipped it.
    // A half-written row is ordinary, and one of them should not cost the other twelve.
    assert_eq!(number(UNSET, "found"), 1.0);
}

const NOTHING_ANYWHERE: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string kind (label="")
            int n (label="") = 0
        }
    }

    list Rows (entry=<Row>) {
        - {
            - n = 1
        }
        - {
            - n = 2
        }
    }

    int found (label="") = $(/t/Rows.filter(|x| x/kind == "here").map(|x| x/n).sum())
    int missing_field (label="") = $(/t/Rows.filter(|x| x/nonexistent == "here").map(|x| x/n).sum())
}
"#;

#[test]
fn a_predicate_no_row_can_answer_matches_no_rows() {
    // The other side of the same decision, stated so it is a choice on the record rather than a
    // side effect. No row holds a `kind`, and none names a field called `nonexistent` at all -
    // both now read as "nothing matched" rather than as an error.
    //
    // The cost is that a misspelt field name is quiet. It buys consistency with the filter that
    // resolves a node, which has always answered this way.
    assert_eq!(number(NOTHING_ANYWHERE, "found"), 0.0);
    assert_eq!(number(NOTHING_ANYWHERE, "missing_field"), 0.0);
}

/// The shape the rules in tasks.os ask the history: one condition the index can answer - a field
/// held equal to a string - and others it cannot. The index narrows the list to the rows holding
/// the string and only those are asked the rest; whatever it answers has to be what the walk did.
const HISTORY: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Record (layout="horizontal") {
            string rule (label="") = ""
            bool failed (label="") = false
            int minutes (label="") = 0
            string kind (label="") = ""
        }
        div Rule (layout="horizontal") {
            string handle (label="") = ""
            int done_count (label="") =
                $(/t/History.filter(|h| h/rule == ../handle && h/failed == false).count())
            int earliest (label="") = $(done_count == 0 ? 0 :
                /t/History.filter(|h| h/rule == ../handle && h/failed == false).map(|h| h/minutes).min())
            int other_way_round (label="") =
                $(/t/History.filter(|h| h/failed == false && h/rule == ../handle).count())
            int three_conditions (label="") =
                $(/t/History.filter(|h| h/rule == ../handle && h/failed == false && h/minutes > 15).count())
            int two_strings (label="") =
                $(/t/History.filter(|h| h/rule == ../handle && h/kind == "x").count())
        }
    }

    list History (entry=<Record>) {
        - {
            - rule = "a"
            - minutes = 10
            - kind = "x"
        }
        - {
            - rule = "a"
            - failed = true
            - minutes = 5
            - kind = "x"
        }
        - {
            - rule = "b"
            - minutes = 20
            - kind = "x"
        }
        - {
            - rule = "a"
            - minutes = 30
            - kind = "y"
        }
        - {
            - rule = ""
            - minutes = 1
            - kind = "x"
        }
    }

    list Rules (entry=<Rule>, key="handle") {
        - {
            - handle = "a"
        }
        - {
            - handle = "b"
        }
        - {
            - handle = "c"
        }
    }
}
"#;

/// A field of the rule whose handle is `handle`, worked out.
fn of_rule(source: &str, handle: &str, field: &str) -> f64 {
    let nodes = app_api::load_document(source.to_string()).expect("document did not load");
    let rules = find(&nodes, "Rules").expect("no Rules");
    let rule = rules
        .children
        .iter()
        .find(|r| {
            find(&r.children, "handle").and_then(|h| h.parameters.get("value"))
                == Some(&OverseerValue::String(handle.to_string()))
        })
        .unwrap_or_else(|| panic!("no rule `{handle}`"));
    match find(&rule.children, field).and_then(|f| f.parameters.get("_computed_value")) {
        Some(OverseerValue::Integer(i)) => *i as f64,
        Some(OverseerValue::Float(f)) => *f,
        other => panic!("`{field}` of `{handle}` is {other:?}"),
    }
}

#[test]
fn several_conditions_find_the_rows_all_of_them_hold() {
    // `a` has two done and one failed: the failed one is left out, so the index did not answer
    // for the whole predicate.
    assert_eq!(of_rule(HISTORY, "a", "done_count"), 2.0);
    assert_eq!(of_rule(HISTORY, "a", "earliest"), 10.0);
    assert_eq!(of_rule(HISTORY, "b", "done_count"), 1.0);
    assert_eq!(of_rule(HISTORY, "b", "earliest"), 20.0);
    assert_eq!(of_rule(HISTORY, "c", "done_count"), 0.0);
    assert_eq!(of_rule(HISTORY, "c", "earliest"), 0.0);
}

#[test]
fn the_condition_the_index_answers_can_come_anywhere() {
    assert_eq!(of_rule(HISTORY, "a", "other_way_round"), 2.0);
    assert_eq!(of_rule(HISTORY, "a", "three_conditions"), 1.0);
    assert_eq!(of_rule(HISTORY, "b", "three_conditions"), 1.0);
}

#[test]
fn what_is_left_after_narrowing_is_not_looked_up_again() {
    // `h/kind == "x"` is a question the index could answer too - but about the whole list, and by
    // then the list is the three rows of `a`. Asked anyway, it named rows by where they stand in
    // the whole list and found them in the short one: three for `a` instead of two.
    assert_eq!(of_rule(HISTORY, "a", "two_strings"), 2.0);
    assert_eq!(of_rule(HISTORY, "b", "two_strings"), 1.0);
}

#[test]
fn several_conditions_over_a_field_holding_a_number_still_walk() {
    // One record holds the rule as a number. The walk matches 5 against "5"; an index of strings
    // would not hold it at all, so it declines and the walk answers.
    let mixed = HISTORY
        .replace("            - rule = \"\"\n", "            - rule = 5\n")
        .replace("            - handle = \"c\"\n", "            - handle = \"5\"\n");
    assert!(mixed.contains("- rule = 5"), "the record was not changed");
    assert_eq!(of_rule(&mixed, "5", "done_count"), 1.0);
    assert_eq!(of_rule(&mixed, "a", "done_count"), 2.0);
}

#[test]
fn several_conditions_reach_a_node_through_the_index() {
    // The other way a filter is answered: as the node a path goes on into.
    let source = CATALOGUE.replace(
        "    string wanted (label=\"\") = \"milk\"\n",
        "    string wanted (label=\"\") = \"milk\"\n    float big_beef (label=\"\") = $(/t/Catalog.filter(|x| x/per_100g > 5 && x/handle == \"beef\")/per_100g)\n    float big_oats (label=\"\") = $(/t/Catalog.filter(|x| x/handle == \"oats\" && x/per_100g > 5)/per_100g)\n",
    );
    assert!(source.contains("big_beef"), "the fields were not added");
    assert_eq!(number(&source, "big_beef"), 26.0);
    assert_eq!(number(&source, "big_oats"), 13.0);
}
