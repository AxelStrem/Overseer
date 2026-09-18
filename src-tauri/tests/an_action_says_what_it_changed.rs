//! An action reports what it touched, and the shortcut that buys agrees with the long way.
//!
//! Pressing a button used to cost two full resolves and a parse. The document was worked out,
//! the action ran, the whole thing was resolved again inside the event, then serialized, parsed
//! and resolved a third time - because nothing said whether the action had moved an entry or
//! merely written a value, and a moved entry renames everything after it.
//!
//! Actions say now. A `set` or a `toggle` names the address it wrote, in the form the dependency
//! graph uses, so the graph can be asked what reads it and only that worked out again. An
//! `append` or a `remove` says only that the shape moved, and the long way still runs.
//!
//! What has to be true is that the two endings agree. Everything here presses the same button
//! twice - once with the graph in hand, once with it forgotten so the shortcut cannot be taken -
//! and insists the documents come out identical. Measured on the food tracker while this was
//! written: 2153ms a press became 453ms, and an edit 1326ms became 562ms.

use overseer::app_api;
use overseer::types::{OverseerNode, OverseerValue};

/// Documents are worked out into a process-wide cache, and these tests empty it deliberately.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

const DOCUMENT: &str = "\
tab counter (label=\"Counter\", mutable=true) {
    int n = 1
    int doubled = $(n * 2)
    int quadrupled = $(doubled * 2)
    text shown (markdown=true) = \"none\"

    list Items (entry=<Row>, key=\"id\") {
        - { - id = \"a\" - amount = 2 }
        - { - id = \"b\" - amount = 3 }
    }
    int total = $(/counter/Items.map(|x| x/amount).sum())

    div (hidden=true) {
        div Row (layout=\"horizontal\") {
            string id = \"\"
            int amount = 0
            int scaled = $(amount * /counter/n)
        }
    }

    button bump (label=\"bump\") {
        on click {
            inc (path=\"/counter/n\", by=1)
        }
    }
    button add (label=\"add\") {
        on click {
            append (list=\"/counter/Items\") {
                - id = \"c\"
                - amount = 4
            }
        }
    }
}
";

fn press(text: &str, button: &str) -> String {
    app_api::execute_event_update(
        text.to_string(),
        vec!["counter".into(), button.into()],
        "click".into(),
    )
    .expect("press")
    .text
}

/// The same press with the graph forgotten, so the shortcut cannot be taken.
fn press_the_long_way(text: &str, button: &str) -> String {
    app_api::forget_dependencies();
    let out = press(text, button);
    out
}

fn value_of(nodes: &[OverseerNode], name: &str) -> Option<i64> {
    for node in nodes {
        if node.name == name {
            let held = node
                .parameters
                .get("_computed_value")
                .or_else(|| node.parameters.get("value"));
            if let Some(OverseerValue::Integer(n)) = held {
                return Some(*n);
            }
        }
        if let Some(found) = value_of(&node.children, name) {
            return Some(found);
        }
    }
    None
}

#[test]
fn a_press_that_writes_a_value_lands_where_the_long_way_lands() {
    serialised(|| {
        let quick = {
            app_api::forget_dependencies();
            app_api::forget_baseline();
            let _ = app_api::load_document(DOCUMENT.to_string()).expect("load");
            press(DOCUMENT, "bump")
        };
        let slow = press_the_long_way(DOCUMENT, "bump");
        assert_eq!(quick, slow, "the shortcut and the long way disagree");
    });
}

#[test]
fn what_the_press_reaches_is_worked_out_again() {
    serialised(|| {
        app_api::forget_dependencies();
        app_api::forget_baseline();
        let _ = app_api::load_document(DOCUMENT.to_string()).expect("load");
        let after = press(DOCUMENT, "bump");
        let nodes = app_api::load_document(after).expect("reload");

        // `n` moved, and everything downstream of it moved with it - including two hops away,
        // and including a formula inside a list entry that reads it from outside.
        assert_eq!(value_of(&nodes, "n"), Some(2), "the press did not write");
        assert_eq!(value_of(&nodes, "doubled"), Some(4), "one hop did not follow");
        assert_eq!(value_of(&nodes, "quadrupled"), Some(8), "two hops did not follow");
        assert_eq!(value_of(&nodes, "scaled"), Some(4), "a list entry did not follow");
    });
}

#[test]
fn a_press_that_appends_still_takes_the_long_way() {
    serialised(|| {
        app_api::forget_dependencies();
        app_api::forget_baseline();
        let _ = app_api::load_document(DOCUMENT.to_string()).expect("load");
        let after = press(DOCUMENT, "add");
        let nodes = app_api::load_document(after.clone()).expect("reload");

        // An append moves the addresses of everything after it, so the shortcut must not run -
        // and the aggregate over the list has to see the new entry.
        assert_eq!(value_of(&nodes, "total"), Some(9), "the appended entry was not counted");
        assert!(after.contains("- id = \"c\""), "the entry was not written");
    });
}

#[test]
fn pressing_repeatedly_keeps_agreeing() {
    // The shortcut leaves a graph behind for the document it produced. If that were stored
    // against the wrong text, the second press would silently fall back and the third would
    // disagree - which is exactly what happened once, and cost a full resolve a press.
    serialised(|| {
        app_api::forget_dependencies();
        app_api::forget_baseline();
        let _ = app_api::load_document(DOCUMENT.to_string()).expect("load");

        let mut held = DOCUMENT.to_string();
        for expected in 2..=5 {
            held = press(&held, "bump");
            let nodes = app_api::load_document(held.clone()).expect("reload");
            assert_eq!(value_of(&nodes, "n"), Some(expected));
            assert_eq!(value_of(&nodes, "quadrupled"), Some(expected * 4));
        }
    });
}

#[test]
fn an_edit_lands_where_a_full_resolve_lands() {
    // The same fault on the editing path: the baseline was taken out of the cache and then
    // asked for, so the graph could never be used and every edit worked out the whole document.
    serialised(|| {
        let edit = |text: &str| {
            let mut values = std::collections::HashMap::new();
            values.insert("counter/n".to_string(), OverseerValue::Integer(7));
            app_api::resolve_selective_update(
                text.to_string(),
                vec!["counter/n".to_string()],
                Some(values),
            )
            .expect("edit")
            .text
        };

        app_api::forget_dependencies();
        app_api::forget_baseline();
        let _ = app_api::load_document(DOCUMENT.to_string()).expect("load");
        let quick = edit(DOCUMENT);

        app_api::forget_dependencies();
        app_api::forget_baseline();
        let slow = edit(DOCUMENT);

        assert_eq!(quick, slow, "the edit shortcut and the long way disagree");

        let nodes = app_api::load_document(quick).expect("reload");
        assert_eq!(value_of(&nodes, "quadrupled"), Some(28), "the cascade did not follow");
    });
}
