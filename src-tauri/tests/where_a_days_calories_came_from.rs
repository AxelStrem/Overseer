//! A day's calories, cut by what the food was rather than what it was made of.
//!
//! Two pies sit side by side on a day. The first asks what the energy was made of - protein, fat,
//! carbs - and the second asks what it was made from: vegan, vegetarian, meat, everything else.
//! The point of the second one is reduction, so what matters is that a wedge shrinking week on
//! week means what it appears to.
//!
//! It is deliberately inexact. A portion of beef is not entirely beef, and a tagged food puts all
//! of its calories into one slice. That is wrong in detail and right enough for the question -
//! whether this is going up or down - and it is the reason these tests check the shares add up
//! rather than checking any share against a nutritional truth.
//!
//! The arithmetic that must hold: `vegetarian` is strict, so no food is counted twice, and the
//! four slices always come to the day's total, so the pie is never a lie about proportions.

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

/// A field of the tab itself, not of anything inside it.
///
/// Scoped on purpose: `calories` is both a day's total and a field on the meal template, and a
/// search that looks everywhere finds the template's - whose lookup fails, because a template has
/// no food. That read as "invalid formula error" and looked like a fault in the sums.
fn number(nodes: &[OverseerNode], field: &str) -> f64 {
    let tab = nodes.iter().find(|n| n.name == "t").expect("no tab");
    tab.children
        .iter()
        .find(|c| c.name == field)
        .and_then(|n| {
            n.parameters
                .get("_computed_value")
                .or_else(|| n.parameters.get("value"))
        })
        .map(|v| match v {
            OverseerValue::Float(f) => *f,
            OverseerValue::Integer(i) => *i as f64,
            other => panic!("{field} is not a number: {other:?}"),
        })
        .unwrap_or_else(|| panic!("no field named {field}"))
}

/// The real arrangement in miniature: a catalogue, meals that look their tags up from it, and the
/// four sums. One food of each interesting kind - vegan, vegetarian-but-not-vegan, meat, a tag
/// that is none of the three, and one nobody has classified.
const DAY: &str = r##"
tab t (label="T") {
    div (hidden=true) {
        div Food (layout="horizontal") {
            string handle (label="") = ""
            tags labels (label="") = ""
            float kcal (label="") = 0
        }
        div Meal (layout="horizontal") {
            string food (label="") = ""
            div (layout="horizontal") {
                tags labels (label="", mutable=false) = $(/t/Catalog.filter(|x| x/handle == ../../food)/labels)
                float calories (label="") = $(/t/Catalog.filter(|x| x/handle == ../../food)/kcal)
            }
        }
    }

    list Catalog (entry=<Food>, key="handle") {
        - {
            - handle = "oats"
            - labels = "vegetarian, vegan"
            - kcal = 100
        }
        - {
            - handle = "cheese"
            - labels = "vegetarian, dairy"
            - kcal = 200
        }
        - {
            - handle = "beef"
            - labels = "meat"
            - kcal = 400
        }
        - {
            - handle = "cod"
            - labels = "fish"
            - kcal = 50
        }
        - {
            - handle = "mystery"
            - kcal = 30
        }
    }

    list intake (entry=<Meal>) {
        - {
            - food = "oats"
        }
        - {
            - food = "cheese"
        }
        - {
            - food = "beef"
        }
        - {
            - food = "cod"
        }
        - {
            - food = "mystery"
        }
    }

    float calories (label="") = $(intake.map(|x| x/calories).sum())
    float vegan_calories (label="") = $(intake.filter(|x| x/labels.filter(|tag| tag == "vegan").count() > 0).map(|x| x/calories).sum())
    float vegetarian_calories (label="") = $(intake.filter(|x| x/labels.filter(|tag| tag == "vegetarian").count() > 0 && x/labels.filter(|tag| tag == "vegan").count() == 0).map(|x| x/calories).sum())
    float meat_calories (label="") = $(intake.filter(|x| x/labels.filter(|tag| tag == "meat").count() > 0).map(|x| x/calories).sum())
    float other_calories (label="") = $(calories - vegan_calories - vegetarian_calories - meat_calories)
}
"##;

fn day() -> Vec<OverseerNode> {
    app_api::load_document(DAY.to_string()).expect("document did not load")
}

#[test]
fn calories_are_grouped_by_what_the_food_was() {
    let nodes = day();
    assert_eq!(number(&nodes, "calories"), 780.0);
    assert_eq!(number(&nodes, "vegan_calories"), 100.0);
    assert_eq!(number(&nodes, "meat_calories"), 400.0);
}

#[test]
fn vegetarian_means_vegetarian_and_not_vegan() {
    // The cheese only. Every vegan food carries `vegetarian` as well, so without the second half
    // of that test the oats would appear in both slices and the pie would come to more than the
    // day - which is the one thing a pie must never do.
    assert_eq!(number(&day(), "vegetarian_calories"), 200.0);
}

#[test]
fn a_food_that_is_none_of_the_three_lands_in_the_rest() {
    // The cod at 50 and the unclassified food at 30. Fish is not meat here, and neither is a
    // food nobody has tagged - calling either of them meat would be inventing a reading.
    assert_eq!(number(&day(), "other_calories"), 80.0);
}

#[test]
fn the_four_shares_come_to_the_day() {
    let nodes = day();
    let parts = number(&nodes, "vegan_calories")
        + number(&nodes, "vegetarian_calories")
        + number(&nodes, "meat_calories")
        + number(&nodes, "other_calories");
    assert!(
        (parts - number(&nodes, "calories")).abs() < 0.001,
        "the shares come to {parts} against a day of {}",
        number(&nodes, "calories")
    );
}

#[test]
fn the_real_document_draws_four_slices_that_add_up() {
    // Through the document people actually read, where the pie is inside a day inside a windowed
    // history - so this also checks that the chart resolves for the days in view.
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let Ok(source) = std::fs::read_to_string(here.join("../examples/weight_tracker/tracker_v2.os"))
    else {
        return;
    };
    overseer::docmgr::manager::DocumentManager::set_current_document(Some(
        here.join("../examples/weight_tracker/tracker_v2.os")
            .to_string_lossy()
            .as_ref(),
    ));
    overseer::source_registry::SourceRegistry::reset();
    let nodes = app_api::load_document(source).expect("tracker did not load");

    fn deep<'a>(node: &'a OverseerNode, name: &str) -> Option<&'a OverseerNode> {
        for c in &node.children {
            if c.name == name {
                return Some(c);
            }
            if let Some(f) = deep(c, name) {
                return Some(f);
            }
        }
        None
    }

    let history = find(&nodes, "History").expect("no History");
    let mut days = 0;
    for entry in &history.children {
        if entry.parameters.contains_key("_out_of_view") {
            continue;
        }
        let chart = deep(entry, "day_sources").expect("a day in view has no sources chart");
        let slices: Vec<f64> = chart
            .children
            .iter()
            .map(|plot| {
                match plot
                    .parameters
                    .get("_computed_amount")
                    .or_else(|| plot.parameters.get("amount"))
                {
                    Some(OverseerValue::Float(f)) => *f,
                    Some(OverseerValue::Integer(i)) => *i as f64,
                    other => panic!("plot {} has no amount: {other:?}", plot.name),
                }
            })
            .collect();
        assert_eq!(slices.len(), 4, "expected four slices, got {slices:?}");

        let totals = deep(entry, "totals").expect("a day has no totals");
        let whole = match totals
            .children
            .iter()
            .find(|c| c.name == "calories")
            .or_else(|| deep(totals, "calories"))
            .and_then(|c| c.parameters.get("_computed_value"))
        {
            Some(OverseerValue::Float(f)) => *f,
            Some(OverseerValue::Integer(i)) => *i as f64,
            other => panic!("a day has no total: {other:?}"),
        };
        let summed: f64 = slices.iter().sum();
        assert!(
            (summed - whole).abs() < 0.01,
            "the slices come to {summed} against a day of {whole}"
        );
        days += 1;
    }
    assert!(days > 0, "no day was in view to check");
    overseer::docmgr::manager::DocumentManager::set_current_document(None);
}
