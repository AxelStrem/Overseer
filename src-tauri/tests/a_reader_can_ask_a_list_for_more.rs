//! A reader can ask a windowed list for more of itself, and it is theirs alone.
//!
//! A window is what the document reads by default. Somebody wanting to look further back is not a
//! change to the document: it is kept against their session, and the file, the bot and every other
//! page still see the window the document states.

use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use serde_json::{json, Value};

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

/// Ten days, newest first, three of them in view.
fn log() -> String {
    let mut text = String::from(
        "tab t (label=\"T\", mutable=true) {\n\
         \x20   div (hidden=true) {\n\
         \x20       div Day (layout=\"horizontal\") {\n\
         \x20           int day (label=\"\") = 0\n\
         \x20           int size (label=\"\") = 0\n\
         \x20           int doubled (label=\"\") = $(size * 2)\n\
         \x20       }\n\
         \x20   }\n\
         \x20   list Days (entry=<Day>, window=3, sort_by=$(|x| 0 - x/day)) {\n",
    );
    for i in 1..=10 {
        text.push_str(&format!(
            "\x20       - {{\n\
             \x20           - day = {i}\n\
             \x20           - size = {i}\n\
             \x20       }}\n"
        ));
    }
    text.push_str("\x20   }\n}\n");
    text
}

fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_more_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("p.os"), log()).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("p.os")).expect("read")
}

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

fn field(entry: &OverseerNode, name: &str) -> Option<OverseerValue> {
    entry
        .children
        .iter()
        .find(|c| c.name == name)
        .and_then(|c| {
            c.parameters
                .get("_computed_value")
                .or_else(|| c.parameters.get("value"))
                .cloned()
        })
}

/// The days drawn, newest first by their number, with what each worked out for `doubled`.
fn in_view(nodes: &[OverseerNode]) -> Vec<(i64, Option<OverseerValue>)> {
    let list = find(nodes, "Days").expect("no Days list");
    let mut days: Vec<(i64, Option<OverseerValue>)> = list
        .children
        .iter()
        .filter(|c| !overseer::resolver::out_of_view(c))
        .filter_map(|c| match field(c, "day") {
            Some(OverseerValue::Integer(d)) => Some((d, field(c, "doubled"))),
            _ => None,
        })
        .collect();
    days.sort_by(|a, b| b.0.cmp(&a.0));
    days
}

fn left_out(nodes: &[OverseerNode]) -> i64 {
    match find(nodes, "Days").and_then(|l| l.parameters.get("_left_out_of_view")) {
        Some(OverseerValue::Integer(n)) => *n,
        _ => 0,
    }
}

fn numbers(days: &[(i64, Option<OverseerValue>)]) -> Vec<i64> {
    days.iter().map(|(d, _)| *d).collect()
}

fn ask_for_more(service: &DocumentRoot, session: &str) -> Value {
    service
        .command_for(
            session,
            Some("p.os"),
            "show_more_of_overseer_list",
            &json!({ "path": "p.os", "list_path": ["t", "Days"] }),
        )
        .expect("asking for more was refused")
}

#[test]
fn asking_for_more_shows_another_window_worked_out() {
    serialised(|| {
        let root = a_root("another");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = service.open_for("alice", "p.os").expect("open");
        assert_eq!(numbers(&in_view(&before)), vec![10, 9, 8]);
        assert_eq!(left_out(&before), 7);

        let answer = ask_for_more(&service, "alice");
        let nodes: Vec<OverseerNode> =
            serde_json::from_value(answer["nodes"].clone()).expect("the answer is the whole document");
        let shown = in_view(&nodes);
        assert_eq!(numbers(&shown), vec![10, 9, 8, 7, 6, 5]);
        assert_eq!(left_out(&nodes), 4);
        for (day, doubled) in &shown {
            assert_eq!(
                doubled.as_ref(),
                Some(&OverseerValue::Integer(day * 2)),
                "day {day} came into view without being worked out"
            );
        }
        assert_eq!(answer["wrote"], json!(false));
    })
}

#[test]
fn asking_for_more_changes_nothing_for_anyone_else() {
    serialised(|| {
        let root = a_root("alone");
        let service = DocumentRoot::new(&root).expect("open the root");
        let text = on_disk(&root);
        ask_for_more(&service, "alice");

        assert_eq!(on_disk(&root), text, "asking for more wrote the file");
        let bob = service.open_for("bob", "p.os").expect("open");
        assert_eq!(numbers(&in_view(&bob)), vec![10, 9, 8]);
        assert_eq!(left_out(&bob), 7);
        let bot = service.open("p.os").expect("open");
        assert_eq!(numbers(&in_view(&bot)), vec![10, 9, 8]);
        let alice = service.open_for("alice", "p.os").expect("open");
        assert_eq!(numbers(&in_view(&alice)).len(), 6);
    })
}

#[test]
fn asking_again_goes_further_until_nothing_is_left_out() {
    serialised(|| {
        let root = a_root("further");
        let service = DocumentRoot::new(&root).expect("open the root");
        ask_for_more(&service, "alice");
        ask_for_more(&service, "alice");
        let alice = service.open_for("alice", "p.os").expect("open");
        assert_eq!(numbers(&in_view(&alice)).len(), 9);
        assert_eq!(left_out(&alice), 1);

        ask_for_more(&service, "alice");
        let alice = service.open_for("alice", "p.os").expect("open");
        assert_eq!(numbers(&in_view(&alice)).len(), 10);
        assert_eq!(left_out(&alice), 0);
    })
}

#[test]
fn an_edit_after_asking_keeps_what_was_asked_for() {
    serialised(|| {
        let root = a_root("edit");
        let service = DocumentRoot::new(&root).expect("open the root");
        // Opened first by somebody else, so a copy worked out with the document's own window is
        // held for this very text when the reader who asked for more makes a change.
        service.open_for("bob", "p.os").expect("open");
        ask_for_more(&service, "alice");

        let answer = service
            .command_for(
                "alice",
                Some("p.os"),
                "write_overseer_values",
                &json!({
                    "path": "p.os",
                    "values": [{
                        "node_path": ["t", "Days", "Day__6", "size"],
                        "value": { "Integer": 60 }
                    }]
                }),
            )
            .expect("the write was refused");
        assert_eq!(answer["wrote"], json!(true));
        assert!(
            !answer.to_string().contains("_out_of_view"),
            "the edit narrowed the list back: {}",
            answer
        );
        assert!(on_disk(&root).contains("- size = 60"));

        let alice = service.open_for("alice", "p.os").expect("open");
        let shown = in_view(&alice);
        assert_eq!(numbers(&shown), vec![10, 9, 8, 7, 6, 5]);
        assert_eq!(shown[4].1, Some(OverseerValue::Integer(120)));
        let bob = service.open_for("bob", "p.os").expect("open");
        assert_eq!(numbers(&in_view(&bob)), vec![10, 9, 8]);
    })
}

#[test]
fn a_list_asked_for_more_saves_back_as_it_was_written() {
    serialised(|| {
        let root = a_root("bytes");
        let service = DocumentRoot::new(&root).expect("open the root");
        ask_for_more(&service, "alice");
        let alice = service.open_for("alice", "p.os").expect("open");
        let written = overseer::file_ops::OverseerFileHandler::serialize_nodes(&alice).expect("serialize");
        assert_eq!(written, log());
    })
}
