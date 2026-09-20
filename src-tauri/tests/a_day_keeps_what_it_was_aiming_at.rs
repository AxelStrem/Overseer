//! A field can take its value once, when the entry is created, and keep it.
//!
//! A day's calorie target falls back to the standing one, so every day in the history reads
//! whatever the standing figure is *now*. Raise the standing target today and yesterday claims
//! to have been aiming at the new number - the record says something that was never true, and
//! the bar drawn against it is measuring the day by a rule it was not kept under.
//!
//! No formula can say "the figure as it stood when this began", because a formula is re-read
//! every time it is looked at. So `freeze=true` writes the value in when the entry is created
//! and the field states it from then on, like any other value somebody typed.
//!
//! Marked by the creating and honoured by the resolver, which is where the value has just been
//! worked out - and by both of the resolver's paths, the full one and the selective one, since
//! either can be the pass that first materialises a new entry.
//!
//! Once, though the mark is left where it is: a field that has been frozen states a value, and a
//! field that states a value is never frozen again. The mark is internal, so it never reaches
//! the file and is gone the next time the document is read.
//!
//! This was blocked until recently for a reason worth remembering - a write to a field nested
//! inside a template entry reported success and changed nothing, which is `silentwrite`. Freezing
//! writes to exactly such a field, so the same care is taken here: the value is stated the way an
//! edit states it, or the serializer treats it as the template's and leaves it out.

use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};

const DOCUMENT: &str = r#"tab t (label="T", mutable=true) {

    int standing = 1800

    div (hidden=true) {

        div Day (layout="vertical") {
            timestamp date (precision="day") = "2026-01-01T00:00:00Z"
            int eaten = 0
            div targets (label="Targets") {
                int target (label="Target", freeze=true, fallback=$(/t/standing)) = null
                int loose (label="Loose", fallback=$(/t/standing)) = null
            }
        }
    }

    list History (entry=<Day>, key="date", keyPrecision="day") { }
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_frozen_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("t.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("t.os")).expect("read")
}

/// A day, added the way the bot adds one.
fn add_a_day(service: &DocumentRoot, date: &str) {
    let mut fields = std::collections::HashMap::new();
    fields.insert("date".to_string(), OverseerValue::String(date.to_string()));
    service.append_at("t.os", "t/History", &fields).expect("append a day");
}

/// What a field of that day reads, worked out as the page would see it.
fn reads(root: &std::path::Path, date: &str, field: &str) -> String {
    let text = on_disk(root);
    let nodes = overseer::app_api::load_document(text).expect("resolve");
    let address = format!("t/History/[{}]/targets/{}", date, field);
    let node = overseer::addressing::find(&nodes, &address)
        .unwrap_or_else(|| panic!("nothing at {}", address));
    // What the field effectively reads: its own value when it states one, and the fallback when
    // it does not - a stated `null` is the absence of a value, not the answer to it.
    let states_one = !matches!(node.parameters.get("value"), None | Some(OverseerValue::Null));
    let held = if states_one {
        node.parameters
            .get("_computed_value")
            .or_else(|| node.parameters.get("value"))
    } else {
        node.parameters
            .get("_computed_fallback")
            .or_else(|| node.parameters.get("value"))
    };
    match held {
        Some(OverseerValue::Integer(n)) => n.to_string(),
        other => format!("{:?}", other),
    }
}

fn raise_the_standing_figure(root: &std::path::Path, to: i64) {
    let service = DocumentRoot::new(root).expect("open the root");
    service
        .set_at("t.os", "t/standing", OverseerValue::Integer(to))
        .expect("raise it");
}

#[test]
fn a_new_entry_takes_the_figure_as_it_stands() {
    serialised(|| {
        let root = a_root("takes");
        let service = DocumentRoot::new(&root).expect("open the root");
        add_a_day(&service, "2026-03-01T00:00:00Z");
        assert!(
            on_disk(&root).contains("- target = 1800"),
            "the day did not take the standing figure:\n{}",
            on_disk(&root)
        );
    });
}

#[test]
fn and_keeps_it_when_the_figure_moves() {
    // The whole point. The day was kept under 1800 and goes on saying so.
    serialised(|| {
        let root = a_root("keeps");
        let service = DocumentRoot::new(&root).expect("open the root");
        add_a_day(&service, "2026-03-01T00:00:00Z");
        raise_the_standing_figure(&root, 2200);

        assert_eq!(
            reads(&root, "2026-03-01T00:00:00Z", "target"),
            "1800",
            "the day followed the standing figure instead of keeping its own"
        );
    });
}

#[test]
fn a_field_that_does_not_say_to_freeze_still_follows() {
    // Freezing is asked for, not the new default. A field that wants to track the standing
    // figure goes on tracking it.
    serialised(|| {
        let root = a_root("loose");
        let service = DocumentRoot::new(&root).expect("open the root");
        add_a_day(&service, "2026-03-01T00:00:00Z");
        raise_the_standing_figure(&root, 2200);

        assert_eq!(reads(&root, "2026-03-01T00:00:00Z", "loose"), "2200");
        assert!(
            !on_disk(&root).contains("- loose ="),
            "a field that was not asked to freeze was written into the entry:\n{}",
            on_disk(&root)
        );
    });
}

#[test]
fn two_days_created_at_different_figures_keep_their_own() {
    serialised(|| {
        let root = a_root("two");
        let service = DocumentRoot::new(&root).expect("open the root");
        add_a_day(&service, "2026-03-01T00:00:00Z");
        raise_the_standing_figure(&root, 2200);
        let service = DocumentRoot::new(&root).expect("open again");
        add_a_day(&service, "2026-03-02T00:00:00Z");

        assert_eq!(reads(&root, "2026-03-01T00:00:00Z", "target"), "1800", "the older day changed");
        assert_eq!(reads(&root, "2026-03-02T00:00:00Z", "target"), "2200", "the newer day did not take");
    });
}

#[test]
fn freezing_happens_once_and_not_again() {
    // Resolving an already-frozen document must leave it where it is. If the mark survived, a
    // later resolve would take the figure again and the day would quietly follow it after all.
    serialised(|| {
        let root = a_root("once");
        let service = DocumentRoot::new(&root).expect("open the root");
        add_a_day(&service, "2026-03-01T00:00:00Z");
        raise_the_standing_figure(&root, 2200);

        let text = on_disk(&root);
        let nodes = overseer::app_api::load_document(text.clone()).expect("resolve");
        let again =
            overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(again.contains("- target = 1800"), "it was refrozen at the new figure:\n{}", again);
        // The mark is internal and must never be written down: a document carrying it would
        // freeze its fields all over again on being read, at whatever the figure had become.
        assert!(!text.contains("freeze_pending"), "the mark reached the file");
        assert!(
            !nodes_carry_the_mark(&nodes),
            "a document read back from the file carries the mark"
        );
    });
}

fn nodes_carry_the_mark(nodes: &[OverseerNode]) -> bool {
    nodes
        .iter()
        .any(|n| n.parameters.contains_key("_freeze_pending") || nodes_carry_the_mark(&n.children))
}

#[test]
fn the_frozen_value_survives_being_written_and_read_again() {
    // `silentwrite` was exactly this shape - a value written into a field nested inside a
    // template entry, dropped on the way to the file.
    serialised(|| {
        let root = a_root("survives");
        let service = DocumentRoot::new(&root).expect("open the root");
        add_a_day(&service, "2026-03-01T00:00:00Z");

        let text = on_disk(&root);
        let nodes = overseer::app_api::load_document(text.clone()).expect("resolve");
        let written =
            overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(
            written.contains("- target = 1800"),
            "the frozen value did not survive a write:\n{}",
            written
        );
        assert!(written.lines().eq(text.lines()), "the document moved when nothing changed");
    });
}
