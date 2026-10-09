//! The add form of a project offers an id for the next item, which can be kept or typed over.
//!
//! The id box starts from `fresh_key(/project/Items, /project/History)`: a short code that no item,
//! open or closed, holds as its handle. It is worked out from the handles already used rather
//! than at random, so it stays the same while the lists do - a formula is resolved again and
//! again, and a random code would change under the cursor - and moves on once an add changes
//! them. Checked against the template every project copies.

use overseer::app_api;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::types::*;

const TEMPLATE: &str = include_str!("../../examples/projects/project_template.os");

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    FormulaEvaluator::set_time_override(None);
    out
}

fn a_file(tag: &str, text: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_idsuggest_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("p.os");
    std::fs::write(&file, text).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-05T09:00:00Z").unwrap().with_timezone(&chrono::Utc);
    FormulaEvaluator::set_time_override(Some(instant));
    file.to_string_lossy().to_string()
}

fn opened(text: String) -> Vec<OverseerNode> {
    app_api::load_document(text).expect("open")
}

/// What a node works out to, or holds when it works nothing out.
fn worked_out(nodes: &[OverseerNode], address: &str) -> String {
    let node = overseer::addressing::find(nodes, address).unwrap_or_else(|| panic!("no {}", address));
    match node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")) {
        Some(OverseerValue::String(s)) => s.clone(),
        other => panic!("{} is {:?}", address, other),
    }
}

fn suggestion(nodes: &[OverseerNode]) -> String {
    worked_out(nodes, "project/NewTask/handle")
}

fn handles(nodes: &[OverseerNode]) -> Vec<String> {
    ["project/Items", "project/History"]
        .iter()
        .flat_map(|list| {
            let list = overseer::addressing::find(nodes, list).expect("list");
            list.children
                .iter()
                .filter_map(|entry| entry.children.iter().find(|c| c.name == "handle"))
                .filter_map(|h| match h.parameters.get("value") {
                    Some(OverseerValue::String(s)) => Some(s.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn add(file: &str, nodes: &[OverseerNode], typed: &[(&str, &str)]) {
    let button = overseer::addressing::name_path(nodes, "project/NewTask/add").expect("the add button");
    let typed = typed
        .iter()
        .map(|(field, text)| app_api::ValueWrite {
            node_path: overseer::addressing::name_path(nodes, &format!("project/NewTask/{}", field)).expect(field),
            value: OverseerValue::String(text.to_string()),
        })
        .collect();
    app_api::run_event_at(file, "p.os", button, "click".into(), "s", typed, Vec::new()).expect("the press");
}

#[test]
fn the_box_offers_a_short_code_no_item_holds() {
    serialised(|| {
        let nodes = opened(TEMPLATE.to_string());
        let code = suggestion(&nodes);
        assert_eq!(code.len(), 4, "{:?}", code);
        assert!(
            code.chars().all(|c| "abcdefghijkmnpqrstuvwxyz23456789".contains(c)),
            "{:?} has a character that reads as another",
            code
        );
        assert!(!handles(&nodes).contains(&code), "{:?} is already an item", code);
    });
}

#[test]
fn the_same_lists_offer_the_same_code() {
    serialised(|| {
        let first = suggestion(&opened(TEMPLATE.to_string()));
        let again = suggestion(&opened(TEMPLATE.to_string()));
        assert_eq!(first, again);
    });
}

#[test]
fn an_add_takes_the_code_and_the_box_offers_another() {
    serialised(|| {
        let file = a_file("takes", TEMPLATE);
        let nodes = opened(std::fs::read_to_string(&file).unwrap());
        let code = suggestion(&nodes);
        add(&file, &nodes, &[("title", "Something to do")]);

        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(&format!("- handle = \"{}\"", code)), "the item was not given {}", code);
        assert!(
            text.contains("$(fresh_key(/project/Items, /project/History))"),
            "the box lost its formula"
        );
        let now = opened(text);
        let next = suggestion(&now);
        assert_ne!(next, code, "the box still offers the id just taken");
        assert!(!handles(&now).contains(&next));
    });
}

#[test]
fn a_typed_id_wins_over_the_suggestion() {
    serialised(|| {
        let file = a_file("typed", TEMPLATE);
        let nodes = opened(std::fs::read_to_string(&file).unwrap());
        let code = suggestion(&nodes);
        add(&file, &nodes, &[("handle", "mine"), ("title", "Something to do")]);

        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("- handle = \"mine\""), "the typed id was not used");
        assert!(!text.contains(&format!("- handle = \"{}\"", code)), "the suggestion was used anyway");
    });
}

const TWO_LISTS: &str = r#"tab project (label="P") {
    div (hidden=true) {
        div Entry {
            string k = ""
        }
    }

    string offered = $(fresh_key(/project/One, /project/Two))

    list One (entry=<Entry>, key="k") {
    }

    list Two (entry=<Entry>, key="k") {
    }
}
"#;

fn with_keys(one: &str, two: &str) -> String {
    let entry = |k: &str| format!("        - {{\n            - k = \"{}\"\n        }}\n", k);
    let opening = |name: &str| format!("list {} (entry=<Entry>, key=\"k\") {{\n", name);
    TWO_LISTS
        .replace(&opening("One"), &format!("{}{}", opening("One"), entry(one)))
        .replace(&opening("Two"), &format!("{}{}", opening("Two"), entry(two)))
}

#[test]
fn the_code_does_not_change_with_the_build() {
    // Pinned, because a hasher that changed between Rust versions would change every suggestion.
    serialised(|| {
        assert_eq!(worked_out(&opened(TWO_LISTS.to_string()), "project/offered"), "kn3y");
    });
}

#[test]
fn a_code_either_list_holds_is_passed_over() {
    // Found by search: holding these two, the first code tried is `upxg` itself, in the second
    // list, so what is offered has to be the next one tried.
    serialised(|| {
        let nodes = opened(with_keys("pad0", "upxg"));
        assert_eq!(worked_out(&nodes, "project/offered"), "5i9z");
    });
}
