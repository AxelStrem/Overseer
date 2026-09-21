//! What a food *is* - vegan, dairy, alcohol - travels to every meal that ate it.
//!
//! The same arrangement as its name and its macros, and for the same reason: a record of eating
//! something stores a handle and an amount, so correcting the catalogue corrects every meal that
//! ever used it. Tags stored on the record instead would be a second copy to keep in step, and
//! they would be wrong the moment a food was reclassified.
//!
//! Two of the tags are not judgements at all. `per_100g` already carries alcohol and caffeine as
//! numbers, so those tags were set from the numbers rather than decided again - and the last test
//! here is what stops the two drifting apart.

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

fn value_of(nodes: &[OverseerNode], field: &str) -> OverseerValue {
    find(nodes, field)
        .and_then(|n| {
            n.parameters
                .get("_computed_value")
                .or_else(|| n.parameters.get("value"))
                .cloned()
        })
        .unwrap_or(OverseerValue::Null)
}

/// A catalogue and something that eats from it, in one document - so these need no files. The
/// mount in the real pair is what makes the catalogue reachable; here the same paths resolve
/// because the lists are simply present.
const DOCUMENT: &str = r##"
tab t (label="T") {
    div (hidden=true) {
        div Label (layout="horizontal") {
            string tag (label="") = ""
            string name (label="") = ""
            string colour (label="") = "#6b7280"
        }
        div Food (layout="horizontal") {
            string handle (label="") = ""
            tags labels (label="", vocabulary="t/Labels") = ""
            float alcohol (label="") = 0
        }
        div Meal (layout="horizontal") {
            string food (label="") = ""
            tags eaten (label="", vocabulary="t/Labels", mutable=false) = $(/t/Catalog.filter(|x| x/handle == ../food)/labels)
        }
    }

    list Labels (entry=<Label>, key="tag") {
        - {
            - tag = "vegan"
            - name = "vegan"
            - colour = "#0e8a16"
        }
        - {
            - tag = "alcohol"
            - name = "alcohol"
            - colour = "#6a1b9a"
        }
    }

    list Catalog (entry=<Food>, key="handle") {
        - {
            - handle = "oats"
            - labels = "vegetarian, vegan"
        }
        - {
            - handle = "beer"
            - labels = "alcohol"
            - alcohol = 4
        }
        - {
            - handle = "mystery"
        }
    }

    list Meals (entry=<Meal>) {
        - {
            - food = "oats"
        }
        - {
            - food = "beer"
        }
        - {
            - food = "mystery"
        }
        - {
            - food = "not_in_the_catalogue"
        }
    }
}
"##;

fn meals() -> Vec<String> {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let meals = find(&nodes, "Meals").expect("no Meals");
    meals
        .children
        .iter()
        .map(|meal| {
            meal.children
                .iter()
                .find(|c| c.name == "eaten")
                .and_then(|c| {
                    c.parameters
                        .get("_computed_value")
                        .or_else(|| c.parameters.get("value"))
                })
                .map(|v| match v {
                    OverseerValue::String(s) => s.clone(),
                    other => format!("{other:?}"),
                })
                .unwrap_or_else(|| "(absent)".into())
        })
        .collect()
}

#[test]
fn a_meal_carries_the_tags_of_the_food_it_ate() {
    let eaten = meals();
    assert_eq!(eaten[0], "vegetarian, vegan");
    assert_eq!(eaten[1], "alcohol");
}

#[test]
fn a_food_nobody_has_classified_yet_says_nothing() {
    // Not an error and not a guess. An untagged food draws no chips, which reads as "nobody has
    // decided" - which is true of seventeen of them, deliberately.
    assert_eq!(meals()[2], "");
}

#[test]
fn a_handle_the_catalogue_does_not_have_reads_as_nothing() {
    // Records with a lost handle exist in the history - three of them, which is why `food` has no
    // default worth eating. What matters most is here: such a record does not resolve into
    // somebody else's tags, and it does not take the document down.
    //
    // It used to read "invalid formula error", on the grounds that a lookup finding nothing is a
    // failed formula. It is not: it is an answered question - there is no such food - and the
    // answer to "what are its tags" is nothing. Fixed where the note here said it should be, at
    // the level of a lookup that finds nothing, so every field that does one reads the same way
    // rather than each being patched where it shows.
    //
    // A field that names something the entry does not have is a different matter and still an
    // error, because that is a mistake in the document rather than a fact about the data.
    assert_eq!(meals()[3], "Null");
}

#[test]
fn the_tags_are_not_stored_on_the_meal() {
    // The whole point of looking them up. If the meal kept its own copy, reclassifying a food
    // would leave every meal that ate it saying the old thing.
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let meals = find(&nodes, "Meals").expect("no Meals");
    let eaten = meals.children[0]
        .children
        .iter()
        .find(|c| c.name == "eaten")
        .expect("no eaten field");
    assert!(
        matches!(eaten.parameters.get("value"), Some(OverseerValue::Formula(_))),
        "the meal stores its tags rather than working them out: {:?}",
        eaten.parameters.get("value")
    );
}

#[test]
fn the_vocabulary_is_a_list_of_its_own() {
    // The renderer turns a handle into a coloured chip by reading this. A missing vocabulary is
    // not fatal - the chips simply lose their colours - so nothing else would notice it going.
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let labels = find(&nodes, "Labels").expect("no vocabulary");
    assert!(labels.children.len() >= 2);
    assert_eq!(value_of(&labels.children[0].children, "tag"), OverseerValue::String("vegan".into()));
}

#[test]
fn the_alcohol_tag_agrees_with_the_number_beside_it() {
    // These two say the same thing twice, which is a thing worth not doing badly. The tag is for
    // reading - a chip on a row - and the number is what anything counting uses. They were set
    // from each other, and this is what keeps them that way.
    //
    // Both catalogues where they are available - the one in the repository, and the deployed one
    // the bot writes to, which is where the hundred and twenty real tags are. The deployed one is
    // outside the repository, so it is checked when present and skipped when not.
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    for path in [
        here.join("../examples/weight_tracker/foods.os"),
        std::path::PathBuf::from(
            "E:/Source/Repos/Secrebot-Docs/personal-stats/documents/foods.os",
        ),
    ] {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        checked += 1;
        let nodes = app_api::load_document(source).expect("catalogue");
        let catalog = find(&nodes, "Catalog").expect("no Catalog");
        check_agreement(catalog);
    }
    assert!(checked > 0, "no catalogue was available to check");
}

fn check_agreement(catalog: &OverseerNode) {

    for food in &catalog.children {
        let handle = match value_of(&food.children, "handle") {
            OverseerValue::String(s) => s,
            _ => continue,
        };
        let tags = match value_of(&food.children, "labels") {
            OverseerValue::String(s) => s,
            _ => String::new(),
        };
        for (field, tag) in [("alcohol", "alcohol"), ("caffeine", "caffeine")] {
            let amount = food
                .children
                .iter()
                .find(|c| c.name == "per_100g")
                .and_then(|block| {
                    block.children.iter().find(|c| c.name == field).and_then(|c| {
                        c.parameters
                            .get("_computed_value")
                            .or_else(|| c.parameters.get("value"))
                    })
                })
                .map(|v| match v {
                    OverseerValue::Float(f) => *f,
                    OverseerValue::Integer(i) => *i as f64,
                    _ => 0.0,
                })
                .unwrap_or(0.0);
            let tagged = tags.split(',').any(|t| t.trim() == tag);
            assert_eq!(
                amount > 0.0,
                tagged,
                "{handle}: per_100g/{field} is {amount} but the `{tag}` tag is {}",
                if tagged { "present" } else { "absent" }
            );
        }
    }
}
