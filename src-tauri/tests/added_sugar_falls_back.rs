//! Added sugar: stated where a food knows it, and two fifths of the total where it does not.
//!
//! The limit is about *free* sugars - what is added, plus honey, syrup and juice - and counts the
//! sugar in whole and dried fruit and the lactose in milk as none of its business. Measured
//! against total sugar the limit says almost nothing: a day with 240 g of dried prunes in it read
//! four times over, and essentially all of that was sugar the guidance excludes.
//!
//! A European label gives "of which sugars" and stops, so most foods will never state this. Hence
//! the fallback. Two things about it are worth holding on to, and are what these check.
//!
//! While every food is unstated, two fifths of everything is arithmetically identical to leaving
//! the bar on total sugar and raising the limit to 125 g. That is deliberate: it costs nothing to
//! adopt, changes no existing entry, and improves one food at a time as figures arrive.
//!
//! And a stated figure must win, including a stated nought - because nought is the truth for
//! fruit, milk, meat and vegetables, and it is exactly where the fallback is most wrong.

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

fn number(nodes: &[OverseerNode], field: &str) -> f64 {
    let tab = nodes.iter().find(|n| n.name == "t").expect("no tab");
    tab.children
        .iter()
        .find(|c| c.name == field)
        .and_then(|n| {
            n.parameters
                .get("_computed_value")
                .or_else(|| n.parameters.get("_computed_fallback"))
                .or_else(|| n.parameters.get("value"))
        })
        .map(|v| match v {
            OverseerValue::Float(f) => *f,
            OverseerValue::Integer(i) => *i as f64,
            other => panic!("{field} is not a number: {other:?}"),
        })
        .unwrap_or_else(|| panic!("no field named {field}"))
}

/// The real chain in miniature: a catalogue with a fallback on one field, meals that reach through
/// it, and the day's two sums.
const DOCUMENT: &str = r##"
tab t (label="T") {
    div (hidden=true) {
        div Food (layout="horizontal") {
            string handle (label="") = ""
            div per_100g (layout="horizontal") {
                float sugar (label="") = 0
                float sugar_added (label="", fallback=$(sugar * 0.4)) = null
            }
        }
        div Meal (layout="horizontal") {
            string food (label="") = ""
            float grams (label="") = 0
            div macros (layout="horizontal") {
                float sugar (label="") = $(grams * /t/Catalog.filter(|x| x/handle == ../food)/per_100g/sugar * 0.01)
                float sugar_added (label="") = $(grams * /t/Catalog.filter(|x| x/handle == ../food)/per_100g/sugar_added * 0.01)
            }
        }
    }

    list Catalog (entry=<Food>, key="handle") {
        - {
            - handle = "prunes"
            div per_100g {
                - sugar = 43
            }
        }
        - {
            - handle = "apple"
            div per_100g {
                - sugar = 10
                - sugar_added = 0
            }
        }
        - {
            - handle = "icecream"
            div per_100g {
                - sugar = 34
                - sugar_added = 32
            }
        }
    }

    list intake (entry=<Meal>) {
        - {
            - food = "prunes"
            - grams = 100
        }
        - {
            - food = "apple"
            - grams = 100
        }
        - {
            - food = "icecream"
            - grams = 100
        }
    }

    float total_sugar (label="") = $(intake.map(|x| x/macros/sugar).sum())
    float added_sugar (label="") = $(intake.map(|x| x/macros/sugar_added).sum())
}
"##;

fn document() -> Vec<OverseerNode> {
    app_api::load_document(DOCUMENT.to_string()).expect("document did not load")
}

fn meal_added(nodes: &[OverseerNode], index: usize) -> f64 {
    let intake = find(nodes, "intake").expect("no intake");
    let macros = intake.children[index]
        .children
        .iter()
        .find(|c| c.name == "macros")
        .expect("no macros");
    macros
        .children
        .iter()
        .find(|c| c.name == "sugar_added")
        .and_then(|c| c.parameters.get("_computed_value"))
        .map(|v| match v {
            OverseerValue::Float(f) => *f,
            OverseerValue::Integer(i) => *i as f64,
            other => panic!("not a number: {other:?}"),
        })
        .expect("no added sugar on the meal")
}

#[test]
fn a_food_that_says_nothing_falls_back_to_two_fifths() {
    // The prunes. 43 g of sugar per 100 g, none of it stated as added, so 17.2.
    assert!((meal_added(&document(), 0) - 17.2).abs() < 0.001);
}

#[test]
fn a_stated_figure_wins_over_the_fallback() {
    // The ice cream states 32 of its 34, and must not be read as 13.6.
    assert!((meal_added(&document(), 2) - 32.0).abs() < 0.001);
}

#[test]
fn a_stated_nought_wins_too() {
    // The case the whole thing turns on. An apple genuinely has no added sugar, and if a written
    // nought were treated as "unset" it would fall back to 4 g - which is the fallback being
    // wrong in exactly the place it is most wrong, and unfixable by filling the figure in.
    assert_eq!(meal_added(&document(), 1), 0.0);
}

#[test]
fn the_fallback_reaches_through_the_lookup() {
    // Not obvious, and the design rests on it: the meal asks the *catalogue* for a field that the
    // catalogue never stated, and gets the catalogue's fallback rather than nothing.
    let nodes = document();
    assert!(number(&nodes, "added_sugar") > 0.0, "the lookup saw no added sugar at all");
}

#[test]
fn two_fifths_of_everything_is_the_same_as_a_limit_of_125() {
    // The claim that makes this worth adopting before any figure is filled in. With nothing
    // stated, `added / 50` and `total / 125` are the same number, so the bar reads exactly as a
    // raised limit would - and every figure added afterwards is an improvement on that, not a
    // change of meaning.
    let only_unstated = DOCUMENT
        .replace("                - sugar_added = 0\n", "")
        .replace("                - sugar_added = 32\n", "");
    let nodes = app_api::load_document(only_unstated).expect("load");
    let total = number(&nodes, "total_sugar");
    let added = number(&nodes, "added_sugar");
    assert!(
        (added / 50.0 - total / 125.0).abs() < 1e-9,
        "added/50 is {} and total/125 is {}",
        added / 50.0,
        total / 125.0
    );
}

#[test]
fn the_real_documents_carry_it_through_to_the_bar() {
    // The catalogue declares it, the meal reads it, the day sums it, and the bar plots it. A
    // break anywhere in that chain leaves the bar reading zero, which looks like a good day.
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
    let mut checked = 0;
    for day in &history.children {
        if day.parameters.contains_key("_out_of_view") {
            continue;
        }
        let totals = deep(day, "totals").expect("a day has no totals");
        let read = |field: &str| -> f64 {
            totals
                .children
                .iter()
                .flat_map(|c| c.children.iter())
                .chain(totals.children.iter())
                .find(|c| c.name == field)
                .and_then(|c| c.parameters.get("_computed_value"))
                .map(|v| match v {
                    OverseerValue::Float(f) => *f,
                    OverseerValue::Integer(i) => *i as f64,
                    _ => f64::NAN,
                })
                .unwrap_or(f64::NAN)
        };
        let total = read("sugar");
        let added = read("sugar_added");
        if total <= 0.0 {
            continue;
        }
        assert!(
            added.is_finite() && added > 0.0,
            "a day with {total} g of sugar reported {added} g added"
        );
        assert!(
            added <= total + 0.001,
            "added sugar {added} exceeds the total {total}"
        );
        // And the bar is pointed at the added figure rather than the total.
        let chart = deep(day, "day_allowances").expect("no allowances chart");
        let plot = chart
            .children
            .iter()
            .find(|p| p.name == "sugar")
            .expect("no sugar bar");
        let plotted = match plot.parameters.get("_computed_amount") {
            Some(OverseerValue::Float(f)) => *f,
            Some(OverseerValue::Integer(i)) => *i as f64,
            other => panic!("the sugar bar has no amount: {other:?}"),
        };
        assert!(
            (plotted - added).abs() < 0.001,
            "the bar plots {plotted} while the day's added sugar is {added}"
        );
        checked += 1;
    }
    assert!(checked > 0, "no day with sugar in it was available to check");
    overseer::docmgr::manager::DocumentManager::set_current_document(None);
}
