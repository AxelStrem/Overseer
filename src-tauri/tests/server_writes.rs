//! Writing to a document from outside the app.
//!
//! This follows what a bot recording a meal actually has to do: find the food, add the day if
//! today has no record yet, add the meal to it, and read back what that came to. The document
//! is a copy, because these tests write.

use overseer::server::{DocumentRoot, RequestError};
use overseer::types::*;
use std::collections::HashMap;

struct Sandbox {
    root: std::path::PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A private copy of the weight tracker, so writing does not disturb the examples.
fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/weight_tracker");
    let root = std::env::temp_dir().join(format!("overseer_write_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for file in ["tracker_v2.os", "foods.os"] {
        std::fs::copy(source.join(file), root.join(file)).unwrap();
    }
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox { root }, documents)
}

fn field(node: &OverseerNode, name: &str) -> Option<OverseerValue> {
    let child = node.children.iter().find(|c| c.name == name)?;
    child
        .parameters
        .get("_computed_value")
        .or_else(|| child.parameters.get("value"))
        .cloned()
}

fn entries(node: &OverseerNode) -> usize {
    node.children.len()
}

#[test]
fn reads_the_food_catalogue_a_bot_chooses_from() {
    let (_sandbox, documents) = sandbox("read");
    let catalogue = documents
        .read_at("foods.os", "food_catalog/Catalog")
        .expect("the catalogue should be readable")
        .node;
    assert!(entries(&catalogue) > 0, "the catalogue came back empty");

    // Addressed by handle, which is what a bot has after matching a description.
    let apple = documents
        .read_at("foods.os", "food_catalog/Catalog/[apple]")
        .expect("apple should be addressable by its handle")
        .node;
    assert!(
        field(&apple, "portion_weight").is_some(),
        "a food should say what one portion weighs"
    );
}

#[test]
fn adds_a_food_the_catalogue_did_not_have() {
    let (_sandbox, documents) = sandbox("newfood");
    let mut fields = HashMap::new();
    fields.insert("handle".to_string(), OverseerValue::String("kiwi".into()));
    fields.insert("name".to_string(), OverseerValue::String("Kiwi fruit".into()));
    fields.insert("portion_weight".to_string(), OverseerValue::Float(75.0));

    let outcome = documents
        .append_at("foods.os", "food_catalog/Catalog", &fields)
        .expect("a food should be addable");
    assert!(
        outcome.node.children.iter().any(|entry| {
            field(entry, "handle") == Some(OverseerValue::String("kiwi".into()))
        }),
        "the new food is not in the catalogue that came back"
    );

    // And it is there on the next read, which is what proves it reached the file.
    let again = documents
        .read_at("foods.os", "food_catalog/Catalog/[kiwi]")
        .expect("the new food should be addressable by its handle")
        .node;
    assert_eq!(
        field(&again, "name"),
        Some(OverseerValue::String("Kiwi fruit".into()))
    );
}

#[test]
fn records_a_meal_against_a_day_that_already_exists() {
    let (_sandbox, documents) = sandbox("meal");
    // The fixture has a record for this day.
    let day = "tracker_v2/History/[2026-08-08]";
    let before = documents
        .read_at("tracker_v2.os", &format!("{}/intake", day))
        .expect("the day should have an intake list")
        .node;

    let mut fields = HashMap::new();
    fields.insert("food".to_string(), OverseerValue::String("apple".into()));
    fields.insert("portions".to_string(), OverseerValue::Float(2.0));
    let outcome = documents
        .append_at("tracker_v2.os", &format!("{}/intake", day), &fields)
        .expect("a meal should be recordable");

    assert_eq!(
        entries(&outcome.node),
        entries(&before) + 1,
        "the meal was not added to the day"
    );

    // The point of reading the result back: a handle that matched nothing would leave the
    // derived figures at their defaults rather than failing, and this is how a caller notices.
    let added = outcome.node.children.last().expect("an entry");
    let calories = added
        .children
        .iter()
        .find(|c| c.name == "macros")
        .and_then(|m| field(m, "calories"));
    assert!(
        calories.is_some(),
        "the recorded meal has no calories, so nothing can be reported back about it"
    );
}

#[test]
fn adds_the_day_when_today_has_no_record_yet() {
    // The app conjures a day record when you edit into an empty day, but that happens in the
    // renderer, so a caller working through this API creates it explicitly. Two ordinary
    // appends rather than a special case.
    let (_sandbox, documents) = sandbox("newday");
    let mut day = HashMap::new();
    day.insert("date".to_string(), OverseerValue::String("2026-09-01".into()));
    documents
        .append_at("tracker_v2.os", "tracker_v2/History", &day)
        .expect("a day should be addable");

    let mut meal = HashMap::new();
    meal.insert("food".to_string(), OverseerValue::String("apple".into()));
    meal.insert("portions".to_string(), OverseerValue::Float(1.0));
    let outcome = documents
        .append_at("tracker_v2.os", "tracker_v2/History/[2026-09-01]/intake", &meal)
        .expect("the new day should accept a meal");
    assert_eq!(entries(&outcome.node), 1);
}

#[test]
fn refuses_to_append_to_something_that_is_not_a_list() {
    let (_sandbox, documents) = sandbox("notalist");
    let outcome = documents.append_at(
        "tracker_v2.os",
        "tracker_v2/History/[2026-08-08]/date",
        &HashMap::new(),
    );
    assert!(
        matches!(outcome, Err(RequestError::Rejected(_))),
        "appending to a field was allowed: {:?}",
        outcome.map(|o| o.address)
    );
}

#[test]
fn writing_leaves_the_document_readable_and_records_what_it_did() {
    let (sandbox, documents) = sandbox("journal");
    let mut fields = HashMap::new();
    fields.insert("handle".to_string(), OverseerValue::String("pear".into()));
    documents
        .append_at("foods.os", "food_catalog/Catalog", &fields)
        .expect("write");

    // The file is still a document, not a half-written one.
    documents
        .open("foods.os")
        .expect("the document should still open after being written");

    let journal = std::fs::read_to_string(sandbox.root.join("overseer-writes.jsonl"))
        .expect("a write should be recorded");
    assert!(journal.contains("\"operation\":\"append\""));
    assert!(journal.contains("pear"), "the journal does not say what was written");
}

fn number(value: Option<OverseerValue>) -> Option<f64> {
    match value? {
        OverseerValue::Float(f) => Some(f),
        OverseerValue::Integer(i) => Some(i as f64),
        _ => None,
    }
}

#[test]
fn the_recorded_meal_carries_the_food_s_own_figures() {
    // The substance of recording a meal: not that a row appeared, but that it means what the
    // catalogue says. A handle matching nothing would leave these at the template's defaults,
    // which is exactly the failure a caller cannot see without checking.
    let (_sandbox, documents) = sandbox("figures");
    let apple = documents
        .read_at("foods.os", "food_catalog/Catalog/[apple]")
        .expect("apple")
        .node;
    let portion = number(field(&apple, "portion_weight")).expect("portion weight");
    let per_100g = apple
        .children
        .iter()
        .find(|c| c.name == "per_100g")
        .expect("per 100 g");
    let calories_per_100g = number(field(per_100g, "calories")).expect("calories per 100 g");

    let mut fields = HashMap::new();
    fields.insert("food".to_string(), OverseerValue::String("apple".into()));
    fields.insert("portions".to_string(), OverseerValue::Float(2.0));
    let outcome = documents
        .append_at(
            "tracker_v2.os",
            "tracker_v2/History/[2026-08-08]/intake",
            &fields,
        )
        .expect("record the meal");

    let added = outcome.node.children.last().expect("the new entry");
    let macros = added
        .children
        .iter()
        .find(|c| c.name == "macros")
        .expect("the entry should have macros");
    let recorded = number(field(macros, "calories")).expect("calories");

    let expected = 2.0 * portion * calories_per_100g * 0.01;
    assert!(
        (recorded - expected).abs() < 0.01,
        "two portions of apple came to {} calories, but the catalogue says {}",
        recorded,
        expected
    );
}

#[test]
fn writing_does_not_disturb_the_rest_of_the_document() {
    // A write re-serializes the whole document, so the parts nobody touched have to come back
    // unchanged - comments above all, since they carry the reasoning and cannot be recomputed.
    let (sandbox, documents) = sandbox("fidelity");
    let path = sandbox.root.join("tracker_v2.os");
    let before = std::fs::read_to_string(&path).unwrap();
    let comments = |text: &str| {
        text.lines()
            .filter(|l| l.trim_start().starts_with("//"))
            .count()
    };

    let mut fields = HashMap::new();
    fields.insert("food".to_string(), OverseerValue::String("apple".into()));
    fields.insert("portions".to_string(), OverseerValue::Float(1.0));
    documents
        .append_at(
            "tracker_v2.os",
            "tracker_v2/History/[2026-08-08]/intake",
            &fields,
        )
        .expect("record the meal");

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        comments(&before),
        comments(&after),
        "the document lost or gained comments when a meal was recorded"
    );
    assert!(
        after.contains("// Calorie tracker, handle-based records"),
        "the document's opening comment did not survive the write"
    );
    // A meal is a few lines; anything more means something else moved.
    let growth = after.lines().count() as i64 - before.lines().count() as i64;
    assert!(
        (1..=6).contains(&growth),
        "recording one meal changed the document by {} lines",
        growth
    );
}

#[test]
fn a_new_food_keeps_the_figures_it_was_given() {
    // A food's calories live one level down, under per_100g. They have to be stated when the
    // entry is created: a field inherited from a template goes back to the template's default
    // on the next resolve unless the entry itself states it, and the default for a calorie
    // figure is a plausible number rather than an obviously missing one. Getting this wrong
    // records a food that looks entirely fine and is wrong, which is the failure this whole
    // arrangement exists to avoid.
    let (_sandbox, documents) = sandbox("nested");
    let mut fields = HashMap::new();
    fields.insert("handle".to_string(), OverseerValue::String("kiwi".into()));
    fields.insert("name".to_string(), OverseerValue::String("Kiwi".into()));
    fields.insert("portion_weight".to_string(), OverseerValue::Float(75.0));
    fields.insert("per_100g/calories".to_string(), OverseerValue::Float(61.0));
    fields.insert("per_100g/protein".to_string(), OverseerValue::Float(1.1));
    documents
        .append_at("foods.os", "food_catalog/Catalog", &fields)
        .expect("the food should be addable");

    let per_100g = documents
        .read_at("foods.os", "food_catalog/Catalog/[kiwi]/per_100g")
        .expect("the new food should have its per-100g figures")
        .node;
    assert_eq!(
        number(field(&per_100g, "calories")),
        Some(61.0),
        "the calories fell back to the template's default, so the food is quietly wrong"
    );
    assert_eq!(number(field(&per_100g, "protein")), Some(1.1));

    // And the figures are what a meal made of it is then computed from.
    let mut meal = HashMap::new();
    meal.insert("food".to_string(), OverseerValue::String("kiwi".into()));
    meal.insert("portions".to_string(), OverseerValue::Float(1.0));
    let outcome = documents
        .append_at(
            "tracker_v2.os",
            "tracker_v2/History/[2026-08-08]/intake",
            &meal,
        )
        .expect("a meal of the new food should record");
    let added = outcome.node.children.last().expect("the new entry");
    let macros = added
        .children
        .iter()
        .find(|c| c.name == "macros")
        .expect("macros");
    let recorded = number(field(macros, "calories")).expect("calories");
    let expected = 75.0 * 61.0 * 0.01;
    assert!(
        (recorded - expected).abs() < 0.01,
        "one portion of the new food came to {} calories, but its own figures imply {}",
        recorded,
        expected
    );
}

#[test]
fn a_list_says_how_to_address_its_entries() {
    // A caller reading the catalogue then wants to address one food. Working the address out
    // for itself would be a second implementation of the rule - and entries of a keyed list
    // are addressed by key, not position, which is exactly the sort of thing two
    // implementations disagree about.
    let (_sandbox, documents) = sandbox("addresses");
    let view = documents
        .read_at("foods.os", "food_catalog/Catalog")
        .expect("the catalogue should be readable");
    assert_eq!(
        view.child_addresses.len(),
        view.node.children.len(),
        "every entry should have been given an address"
    );
    assert!(
        view.child_addresses.contains(&"food_catalog/Catalog/[apple]".to_string()),
        "entries are not addressed by their handle: {:?}",
        view.child_addresses
    );
    // And an address it was handed can be read straight back.
    let apple = documents
        .read_at("foods.os", "food_catalog/Catalog/[apple]")
        .expect("an address the server gave should be one it accepts");
    assert_eq!(
        field(&apple.node, "handle"),
        Some(OverseerValue::String("apple".into()))
    );
}
