//! How a newly materialized list entry is written to disk.
//!
//! When a link proxy shows a phantom preview for a key with no entry, the renderer turns it
//! into a real entry by cloning the template. What matters here is what the serializer then
//! writes: an entry should record only what distinguishes it - its key, and whatever was
//! added to it - not a transcription of the template it came from.

use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

const DOC: &str = r#"div t {
    div (hidden=true) {
        div Meal (layout="vertical") {
            string food = "apple"
            float portions = null
        }

        div Day (layout="vertical") {
            timestamp date (precision="day") = "2026-01-01"

            div totals (layout="horizontal") {
                float calories (label="Total kcal") = $(intake.map(|x| x/portions).sum())
            }

            list intake (entry=<Meal>, layout="vertical")

            // A comment inside the template, of the kind that used to be copied into
            // every materialized entry.
            button add (label="Add") {
                on click {
                    append (list="../intake") {
                        - portions = 1
                    }
                }
            }
        }
    }
    list History (entry=<Day>, key="date", keyPrecision="day") {
        - {
            - date = "2026-08-04"
        }
    }
}
"#;

/// Clone the template the way the renderer does when materializing a phantom, including the
/// provenance strip that keeps it from being written out as a copy of the template.
fn clone_template_as_entry(nodes: &[OverseerNode], template: &str, key_value: &str) -> OverseerNode {
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
    fn strip(n: &mut OverseerNode) {
        n.source_id = None;
        n.source_fingerprint = None;
        n.source_snapshot = None;
        for c in n.children.iter_mut() {
            strip(c);
        }
    }

    let mut item = find(nodes, template).expect("template").clone();
    strip(&mut item);
    item.name = format!("{}__2", template);
    item.parameters.insert(
        "_from_template".to_string(),
        OverseerValue::Boolean(true),
    );
    item.parameters.insert(
        "_original_type".to_string(),
        OverseerValue::String(template.to_string()),
    );
    // The marker the renderer sets on a materialized entry.
    item.parameters.insert(
        "_materialized_from_template".to_string(),
        OverseerValue::Boolean(true),
    );
    if let Some(date) = item.children.iter_mut().find(|c| c.name == "date") {
        date.parameters.insert(
            "value".to_string(),
            OverseerValue::String(key_value.to_string()),
        );
        date.parameters.insert(
            "_override_present".to_string(),
            OverseerValue::Boolean(true),
        );
        date.authored_dash = true;
    }
    item
}

fn entry_text(serialized: &str, key: &str) -> String {
    let start = serialized
        .find(key)
        .unwrap_or_else(|| panic!("no entry for {} in:\n{}", key, serialized));
    let head = serialized[..start].rfind("- {").expect("entry start");
    let mut depth = 0i32;
    let bytes = serialized[head..].char_indices();
    let mut end = serialized.len() - head;
    for (i, ch) in bytes {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                end = i + 1;
                break;
            }
        }
    }
    serialized[head..head + end].to_string()
}

#[test]
fn a_materialized_entry_records_only_what_differs() {
    serialised(|| {
        overseer::source_registry::SourceRegistry::reset();
        let (_r, mut nodes) = parser::parse_document(DOC).expect("parse");
        resolver::resolve_document(&mut nodes);

        let entry = clone_template_as_entry(&nodes, "Day", "2026-08-05");
        {
            let history = nodes[0]
                .children
                .iter_mut()
                .find(|c| c.name == "History")
                .expect("History");
            history.children.insert(0, entry);
        }
        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let text = entry_text(&out, "2026-08-05");

        assert!(
            !text.contains("A comment inside the template"),
            "the template's comments were copied into the entry:\n{}",
            text
        );
        assert!(
            !text.contains("Total kcal"),
            "the template's totals block was copied into the entry:\n{}",
            text
        );
        assert!(
            !text.contains("button add"),
            "the template's button was copied into the entry:\n{}",
            text
        );
        assert!(
            text.contains("2026-08-05"),
            "the entry should record its own key:\n{}",
            text
        );
        assert!(
            text.lines().count() <= 6,
            "a new entry should be a handful of lines, got {}:\n{}",
            text.lines().count(),
            text
        );
    });
}

/// Whatever is added to the entry afterwards must still be written, and inside its list.
#[test]
fn a_materialized_entry_still_records_what_was_added_to_it() {
    serialised(|| {
        overseer::source_registry::SourceRegistry::reset();
        let (_r, mut nodes) = parser::parse_document(DOC).expect("parse");
        resolver::resolve_document(&mut nodes);

        let mut entry = clone_template_as_entry(&nodes, "Day", "2026-08-05");
        {
            let intake = entry
                .children
                .iter_mut()
                .find(|c| c.name == "intake")
                .expect("intake list on the cloned entry");
            let mut meal = OverseerNode {
                name: "Meal__1".to_string(),
                node_type: "Meal".to_string(),
                template: None,
                parameters: Default::default(),
                children: Vec::new(),
                is_hierarchy_transparent: false,
                param_order: Vec::new(),
                raw_value_literal: None,
                authored_dash: true,
                child_original_index: None,
                leading_blank_lines: 0,
                source_snapshot: None,
                source_id: None,
                source_fingerprint: None,
            };
            meal.parameters.insert(
                "_from_template".to_string(),
                OverseerValue::Boolean(true),
            );
            meal.parameters.insert(
                "_original_type".to_string(),
                OverseerValue::String("Meal".to_string()),
            );
            let mut portions = meal.clone();
            portions.name = "portions".to_string();
            portions.node_type = "float".to_string();
            portions.parameters.clear();
            portions
                .parameters
                .insert("value".to_string(), OverseerValue::Integer(1));
            portions.parameters.insert(
                "_override_present".to_string(),
                OverseerValue::Boolean(true),
            );
            meal.children.push(portions);
            intake.children.push(meal);
        }
        {
            let history = nodes[0]
                .children
                .iter_mut()
                .find(|c| c.name == "History")
                .expect("History");
            history.children.insert(0, entry);
        }

        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let text = entry_text(&out, "2026-08-05");

        assert!(
            text.contains("portions = 1"),
            "the added record was not written:\n{}",
            text
        );
        assert!(
            text.contains("intake"),
            "the added record must be written inside its list, not a bare block:\n{}",
            text
        );

        // And the document must still read back with the record in the right place.
        overseer::source_registry::SourceRegistry::reset();
        let (_r2, mut reparsed) = parser::parse_document(&out).expect("reparse");
        resolver::resolve_document(&mut reparsed);
        let history = reparsed[0]
            .children
            .iter()
            .find(|c| c.name == "History")
            .expect("History");
        let day = history
            .children
            .iter()
            .find(|d| {
                d.children.iter().any(|c| {
                    c.name == "date"
                        && matches!(c.parameters.get("value"),
                            Some(OverseerValue::String(s)) if s == "2026-08-05")
                })
            })
            .expect("the new day should survive a reload");
        let records = day
            .children
            .iter()
            .find(|c| c.name == "intake")
            .map(|l| l.children.len())
            .unwrap_or(0);
        assert_eq!(records, 1, "the new day should reload with its one record");
    });
}
