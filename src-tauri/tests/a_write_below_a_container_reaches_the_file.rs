//! A write below a container reaches the file.
//!
//! An entry made from a template stores only what it says for itself; everything else comes from
//! the template. Writing to a field the entry does not mention therefore means recording an
//! override, and the entry had only one way to say one: `- name = value`, which names a child of
//! the entry. A field one container further in - a day's target, inside the div the day groups
//! its targets in - could not be said that way, so the container was skipped when the document
//! was written and the write went with it.
//!
//! The write itself was never lost on the way in. It reached the node, the action reported the
//! address it had written, and the call answered Ok. Only the writing-out dropped it, which is
//! the worst shape this can take: a write that fails loudly costs a retry, one that fails
//! quietly costs the data.
//!
//! The document already had the form needed - the one a nested list uses, naming the container
//! and putting the overrides inside it - so what was missing was only the serializer emitting it.

use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    int standing = 5

    div (hidden=true) {

        div Score (layout="horizontal") {
            float rating = $(/t/standing * 2)
        }

        div Day (layout="vertical") {
            timestamp date (precision="day") = "2026-01-01T00:00:00Z"
            float flat = 1
            div targets (label="Targets") {
                float goal = 10
                div inner (label="I") {
                    float deeper = 20
                }
            }
            div (layout="horizontal") {
                float under_wrapper = 30
            }
            <Score> quality {
            }
        }
    }

    list History (entry=<Day>, key="date", keyPrecision="day") {
        - {
            - date = "2026-09-18T00:00:00Z"
        }
    }
}
"#;

const KEY: &str = "t/History/[2026-09-18T00:00:00Z]";

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_below_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("t.os"), DOCUMENT).expect("write");
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("t.os")).expect("read")
}

/// Write at an address and say what the document holds there afterwards, read back from the file.
fn write_and_read_back(root: &std::path::Path, address: &str, value: f64) -> Option<OverseerValue> {
    let service = DocumentRoot::new(root).expect("open the root");
    service
        .set_at("t.os", address, OverseerValue::Float(value))
        .expect("the write was refused");
    let fresh = DocumentRoot::new(root).expect("open again");
    fresh
        .read_at("t.os", address)
        .ok()
        .and_then(|found| found.node.parameters.get("value").cloned())
}

#[test]
fn a_field_inside_a_named_container_is_written() {
    let root = a_root("named");
    let address = format!("{}/targets/goal", KEY);
    assert_eq!(
        write_and_read_back(&root, &address, 42.0),
        Some(OverseerValue::Integer(42)),
        "the write did not survive being written out:
{}",
        on_disk(&root)
    );
}

#[test]
fn so_is_one_two_containers_down() {
    let root = a_root("deeper");
    let address = format!("{}/targets/inner/deeper", KEY);
    assert_eq!(
        write_and_read_back(&root, &address, 7.0),
        Some(OverseerValue::Integer(7)),
        "a second container was one too many:
{}",
        on_disk(&root)
    );
}

#[test]
fn a_field_the_entry_holds_directly_still_is() {
    // This shape always worked; it is here so a change that fixes the one above by breaking this
    // one does not look like a pass.
    let root = a_root("direct");
    let address = format!("{}/flat", KEY);
    assert_eq!(write_and_read_back(&root, &address, 3.0), Some(OverseerValue::Integer(3)));
}

#[test]
fn a_wrapper_is_still_not_named() {
    // A div that groups for layout stands for nothing and has no name in an address, so what is
    // inside it is written as the entry's own - which is what it did before this and still does.
    let root = a_root("wrapper");
    let address = format!("{}/under_wrapper", KEY);
    assert_eq!(write_and_read_back(&root, &address, 9.0), Some(OverseerValue::Integer(9)));
    let text = on_disk(&root);
    assert!(
        text.contains("- under_wrapper = 9"),
        "the wrapper was named in the entry:
{}",
        text
    );
}

#[test]
fn writing_twice_replaces_rather_than_repeats() {
    // The override block is part of the entry once written, so the second write edits it. If it
    // were appended instead, a field written daily would grow the document without bound.
    let root = a_root("twice");
    let address = format!("{}/targets/goal", KEY);
    write_and_read_back(&root, &address, 42.0);
    assert_eq!(write_and_read_back(&root, &address, 43.0), Some(OverseerValue::Integer(43)));
    assert_eq!(
        on_disk(&root).matches("div targets {").count(),
        1,
        "the entry gained a second block:
{}",
        on_disk(&root)
    );
}

#[test]
fn a_nested_template_instance_keeps_its_own_body() {
    // The trap this fell into once. `quality` stands for an instance of another template, and its
    // children carry the markers that mean "written here" for reasons of their own. Walking into
    // one copies that template's formulas into every entry - saying something the entry never
    // said - so the walk stops at anything that is not a plain container.
    let root = a_root("instance");
    let address = format!("{}/targets/goal", KEY);
    write_and_read_back(&root, &address, 42.0);
    let text = on_disk(&root);
    // Inside the list only - the template's own declaration says these things, and should.
    let entries = &text[text.find("list History").expect("the list")..];
    assert!(
        !entries.contains("standing * 2"),
        "the nested template's own body was copied into the entry:
{}",
        entries
    );
    assert!(!entries.contains("rating"), "the instance was written into the entry:
{}", entries);
}

#[test]
fn nothing_else_in_the_document_moves() {
    // The whole document is rewritten on every save, so a change to how entries are written is a
    // change to every document there is. Only the lines the write is about may differ.
    let root = a_root("quiet");
    let before = on_disk(&root);
    let address = format!("{}/targets/goal", KEY);
    write_and_read_back(&root, &address, 42.0);
    let after = on_disk(&root);

    // The block naming the container, and the value inside it. Nothing else.
    let added: Vec<String> = after
        .lines()
        .filter(|l| !before.lines().any(|b| b == *l))
        .map(|l| l.trim().to_string())
        .collect();
    assert_eq!(
        added,
        vec!["div targets {".to_string(), "- goal = 42".to_string()],
        "more than the write changed"
    );
    let removed: Vec<&str> = before.lines().filter(|l| !after.lines().any(|a| a == *l)).collect();
    assert!(removed.is_empty(), "the write removed lines: {:?}", removed);
}
