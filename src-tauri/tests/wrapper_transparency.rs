//! An unnamed `div` groups nodes for layout and should be invisible to everything else.
//!
//! The renderer already treats one that way. The rest of the system does not, and each place
//! that disagrees is a trap: a document laid out the obvious way silently loses an address, a
//! formula, or an action target. These tests state the rule once, for every layer that has to
//! honour it.

use overseer::actions::ActionExecutor;
use overseer::addressing;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

/// A day with its summary fields grouped for layout, and the list left outside the group.
const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {
    div Day (layout="vertical") {
        div (layout="horizontal", margin=0) {
            timestamp date (precision="day") = "2026-08-09"
            int eaten (label="Eaten") = 3
            button add (label="Add") {
                on click {
                    append (list="../items") {
                        - amount = 1
                    }
                }
            }
        }

        int doubled (label="Doubled") = $(eaten * 2)

        div detail (layout="horizontal", margin=0) {
            int doubled_from_within (label="Deeper") = $(eaten * 2)

            div (layout="horizontal", margin=0) {
                int buried (label="Buried") = 11
            }
        }

        int reached (label="Reached") = $(detail/buried)

        list items (entry=<Item>) {
            - {
                - amount = 5
            }
        }
    }

    div (hidden=true) {
        div Item (layout="horizontal") {
            int amount = 0
        }
    }
}
"#;

fn open() -> Vec<OverseerNode> {
    let (_rest, mut nodes) = parser::parse_document(DOCUMENT).expect("parse");
    resolver::resolve_document(&mut nodes);
    nodes
}

fn value_at(nodes: &[OverseerNode], address: &str) -> Option<OverseerValue> {
    let node = addressing::find(nodes, address)?;
    let path: Vec<String> = address.split('/').map(|s| s.to_string()).collect();
    FormulaEvaluator::get_effective_value_for_node(node, &path, nodes).ok()
}

#[test]
fn a_field_inside_a_wrapper_keeps_its_address() {
    let nodes = open();
    assert!(
        addressing::find(&nodes, "t/Day/date").is_some(),
        "the wrapper swallowed the address of what it groups"
    );
    assert!(
        addressing::find(&nodes, "t/Day/eaten").is_some(),
        "the wrapper swallowed the address of what it groups"
    );
}

#[test]
fn the_wrapper_itself_is_not_addressable() {
    let nodes = open();
    // If it had an address of its own, adding a group for layout would change every address
    // beneath it - which is the thing transparency is for.
    assert!(addressing::find(&nodes, "t/Day/div").is_none());
    assert!(addressing::find(&nodes, "t/Day/div/date").is_none());
}

#[test]
fn a_value_can_be_written_at_an_address_inside_a_wrapper() {
    let mut nodes = open();
    let path = addressing::name_path(&nodes, "t/Day/eaten").expect("a path to the field");
    ActionExecutor::assign_value(
        &mut nodes,
        &format!("/{}", path.join("/")),
        OverseerValue::Integer(7),
    )
    .expect("set the value");
    resolver::resolve_document(&mut nodes);

    assert_eq!(value_at(&nodes, "t/Day/eaten"), Some(OverseerValue::Integer(7)));
}

#[test]
fn a_formula_outside_a_wrapper_sees_what_is_inside_it() {
    let nodes = open();
    // `doubled` is a sibling of the group, and reads `eaten` from within it.
    assert_eq!(
        value_at(&nodes, "t/Day/doubled"),
        Some(OverseerValue::Integer(6)),
        "a formula could not reach a field grouped for layout"
    );
}

#[test]
fn a_formula_one_level_down_still_sees_into_a_wrapper() {
    // The reference starts inside a sibling container, so the search has to climb out of it
    // before looking into the group. This is the shape a meal's macros use: they compute from
    // `grams`, which sits in the row above them.
    let nodes = open();
    assert_eq!(
        value_at(&nodes, "t/Day/detail/doubled_from_within"),
        Some(OverseerValue::Integer(6)),
        "a formula inside a sibling could not reach a field grouped for layout"
    );
}

#[test]
fn a_path_reaches_through_a_wrapper_it_names_no_segment_for() {
    // `detail/buried` names the container and the field, and says nothing about the row the
    // field happens to be arranged in - which is what makes it possible to rearrange one
    // without rewriting every formula that reads it.
    let nodes = open();
    assert_eq!(
        value_at(&nodes, "t/Day/reached"),
        Some(OverseerValue::Integer(11)),
        "a path could not reach through a layout group inside the node it named"
    );
}

#[test]
fn an_action_inside_a_wrapper_reaches_its_parents_siblings() {
    let mut nodes = open();
    let before = addressing::find(&nodes, "t/Day/items")
        .map(|n| n.children.len())
        .expect("the list");

    let path = addressing::name_path(&nodes, "t/Day/add").expect("a path to the button");
    ActionExecutor::execute_event(&mut nodes, &path, "click").expect("press Add");

    let after = addressing::find(&nodes, "t/Day/items")
        .map(|n| n.children.len())
        .expect("the list");
    assert_eq!(
        after,
        before + 1,
        "`../items` from inside a group did not find the list beside the group"
    );
}

// -- the same rule, inside the entries of a list ---------------------------------------------
//
// This is where it first went wrong in practice: grouping a day's summary for layout made
// `History/[2026-08-09]/date` stop resolving, so the write API could no longer see the field
// its own addresses had just handed out.

const KEYED: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div DayRecord (layout="vertical") {
            div (layout="horizontal", margin=0) {
                timestamp date (precision="day") = "2026-01-01"
                int eaten (label="Eaten") = 0
            }
            list items (entry=<Item>)
        }
        div Item (layout="horizontal") {
            int amount = 0
        }
    }

    list History (entry=<DayRecord>, key="date", keyPrecision="day") {
        - {
            - date = "2026-08-09"
            list items {
                - {
                    - amount = 5
                }
            }
        }
    }
}
"#;

fn open_keyed() -> Vec<OverseerNode> {
    let (_rest, mut nodes) = parser::parse_document(KEYED).expect("parse");
    resolver::resolve_document(&mut nodes);
    nodes
}

#[test]
fn an_entry_is_still_keyed_by_a_field_inside_a_wrapper() {
    let nodes = open_keyed();
    assert!(
        addressing::find(&nodes, "t/History/[2026-08-09]").is_some(),
        "the entry lost its key when the key field was grouped for layout"
    );
}

#[test]
fn a_field_of_an_entry_keeps_its_address_through_a_wrapper() {
    let nodes = open_keyed();
    assert!(
        addressing::find(&nodes, "t/History/[2026-08-09]/date").is_some(),
        "the field the entry is keyed by is no longer addressable"
    );
    assert!(
        addressing::find(&nodes, "t/History/[2026-08-09]/items").is_some(),
        "a list beside the wrapper is no longer addressable"
    );
}

#[test]
fn a_field_of_an_entry_can_be_written_through_a_wrapper() {
    let mut nodes = open_keyed();
    let address = "t/History/[2026-08-09]/eaten";
    let path = addressing::name_path(&nodes, address).expect("a path to the field");
    ActionExecutor::assign_value(
        &mut nodes,
        &format!("/{}", path.join("/")),
        OverseerValue::Integer(4),
    )
    .expect("set the value");
    resolver::resolve_document(&mut nodes);
    assert_eq!(value_at(&nodes, address), Some(OverseerValue::Integer(4)));
}

// -- and the file it is all written back to --------------------------------------------------

const WITH_INSTANCE: &str = r#"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Scorer (layout="horizontal", margin=0) {
            int input (hidden=true) = 0
            int out (label="Out") = $(input * 2)
        }
        div Row (layout="vertical") {
            int amount = 1

            div (layout="horizontal", margin=0) {
                <Scorer> quality {
                    - input = $(../../amount)
                }
            }
        }
    }

    list rows (entry=<Row>) {
        - {
            - amount = 3
        }
    }
}
"#;

/// Saving consults a process-wide registry of parsed sources, so two of these running at once
/// see each other's document. The other suites serialise for the same reason.
static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn load_and_save(text: &str) -> String {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    overseer::source_registry::SourceRegistry::reset();
    let nodes = overseer::app_api::load_document(text.to_string()).expect("load");
    let across_ipc: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&nodes).expect("to json")).expect("from json");
    let out = overseer::file_ops::OverseerFileHandler::serialize_nodes(&across_ipc)
        .expect("serialize");
    overseer::app_api::canonicalize_document(&out)
}

#[test]
fn a_wrapper_does_not_change_what_a_record_stores() {
    // An entry keeps what distinguishes it and nothing else. Wrapping the template's fields
    // for layout put the instance's overrides into every record on disk - a two-line entry
    // became eight, and each copy stopped tracking the template it came from.
    let saved = load_and_save(WITH_INSTANCE);
    // Only inside the list: the template above legitimately says `- input =`, which is where
    // that override belongs and the whole point of it not being copied downwards.
    let records = &saved[saved.find("list rows").expect("the list")..];
    assert!(
        !records.contains("- input ="),
        "the record gained the template's own overrides:\n{}",
        records
    );
    assert!(records.contains("- amount = 3"), "the record lost what it did say");
}

#[test]
fn a_document_with_a_wrapper_round_trips() {
    let once = load_and_save(WITH_INSTANCE);
    assert_eq!(once, WITH_INSTANCE, "saving changed the file");
}

#[test]
#[ignore = "diagnostic: prints the shape of a materialized entry"]
fn show_the_entry_shape() {
    let nodes = overseer::app_api::load_document(WITH_INSTANCE.to_string()).expect("load");
    fn show(n: &OverseerNode, depth: usize) {
        let marks: Vec<&str> = ["_override_present", "_explicit_child_override",
                                "_materialized_from_template"]
            .iter().filter(|k| n.parameters.contains_key(**k)).cloned().collect();
        println!("{}{} {:?} transparent={} template={:?} {:?}", "  ".repeat(depth), n.node_type, n.name,
                 n.is_hierarchy_transparent, n.template, marks);
        for c in &n.children { show(c, depth + 1); }
    }
    let rows = addressing::find(&nodes, "t/rows").expect("rows");
    for entry in &rows.children { show(entry, 0); }
}

#[test]
fn appending_puts_a_field_where_the_template_keeps_it() {
    // A caller appends by field name - `{amount: 9}` - and the template may keep that field
    // inside a group. The value has to land on the template's field, not beside it: a stray
    // second `amount` leaves every formula reading the template's default instead.
    let mut nodes = open_keyed();
    let mut overrides = Vec::new();
    let mut field = OverseerNode::new_with_type("-".to_string(), Some("eaten".to_string()));
    field.parameters.insert("value".to_string(), OverseerValue::Integer(9));
    overrides.push(field);

    ActionExecutor::append_entry(&mut nodes, "/t/History", &overrides).expect("append a day");
    resolver::resolve_document(&mut nodes);

    let entry = addressing::find(&nodes, "t/History")
        .and_then(|list| list.children.last())
        .expect("the new entry");
    let named_eaten = |n: &OverseerNode| n.name == "eaten";
    let direct = entry.children.iter().filter(|c| named_eaten(c)).count();
    let through = addressing::effective_children(entry)
        .into_iter()
        .filter(|(_, c)| named_eaten(c))
        .count();
    assert_eq!(
        (direct, through),
        (0, 1),
        "the appended field should sit where the template keeps it, not beside the group"
    );
}

// -- layouts the document may ask for ---------------------------------------------------------

#[test]
fn flow_is_a_layout_the_resolver_keeps() {
    // An unknown layout value is quietly turned into the opposite of its parent's, so a new
    // one has to be admitted here or the document silently gets something else.
    let document = r#"tab t (label="T") {
    list cards (layout="flow") {
        - {
            - amount = 1
        }
    }
}
"#;
    let (_rest, mut nodes) = parser::parse_document(document).expect("parse");
    resolver::resolve_document(&mut nodes);
    let cards = addressing::find(&nodes, "t/cards").expect("the list");
    let effective = cards
        .parameters
        .get("_effective_layout")
        .or_else(|| cards.parameters.get("layout"));
    assert!(
        matches!(effective, Some(OverseerValue::String(s)) if s == "flow"),
        "the resolver replaced the layout with {:?}",
        effective
    );
}
