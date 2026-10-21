//! An answer is described against what the caller holds.
//!
//! A change sent as an instruction is answered with what changed, worked out against the document
//! the server last produced for the file's text. The page applies that by child index to the tree
//! it holds, so the two have to be the same document. They were not once something else wrote in
//! between: the bot's write goes through the same server, which then holds the bot's document
//! under the file's new text, and the page's next edit was described against that - applied to a
//! tree without the bot's entry in it. So the page says which text it is showing, and is answered
//! against that or, when that is not to be had, with the whole document.

use overseer::app_api;
use overseer::delta::{self, DocumentChange};
use overseer::types::*;

const DOCUMENT: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Row (layout="horizontal") {
            string id = ""
            int qty = 0
        }
    }
    list Rows (entry=<Row>, key="id") {
        - {
            - id = "b"
            - qty = 2
        }
        - {
            - id = "c"
            - qty = 3
        }
    }
    int total = $(Rows.sum(|x| x/qty))
}
"#;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A file holding the document, opened the way the page opens it.
fn opened(tag: &str) -> (String, Vec<OverseerNode>) {
    let root = std::env::temp_dir().join(format!("overseer_shown_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("d.os");
    std::fs::write(&file, DOCUMENT).unwrap();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("open");
    (file.to_string_lossy().to_string(), nodes)
}

/// What the page shows of a document: each node's name and value, and its children.
fn shown(nodes: &[OverseerNode]) -> serde_json::Value {
    serde_json::Value::Array(
        nodes
            .iter()
            .map(|n| {
                let value = n.parameters.get("_computed_value").or_else(|| n.parameters.get("value"));
                serde_json::json!({
                    "name": n.name,
                    "value": serde_json::to_value(value).unwrap(),
                    "children": shown(&n.children),
                })
            })
            .collect(),
    )
}

/// The bot records a row, through the same server, knowing nothing of the page.
fn the_bot_appends(path: &str) {
    app_api::append_entry_at(
        path,
        "d.os",
        vec!["t".into(), "Rows".into()],
        [("id".to_string(), OverseerValue::String("a".into())), ("qty".to_string(), OverseerValue::Integer(5))].into_iter().collect(),
        "bot",
    )
    .expect("the bot's append");
}

/// The page writes a field of an entry it holds, named the way it names it: by the entry's name.
fn the_page_writes(path: &str, held: &[OverseerNode]) -> app_api::ResolvedUpdate {
    let mut field = overseer::addressing::name_path(held, "t/Rows/[c]").expect("no entry c");
    field.push("qty".into());
    let w: app_api::ValueWrite = serde_json::from_value(serde_json::json!({
        "node_path": field, "value": OverseerValue::Integer(7)
    }))
    .unwrap();
    app_api::write_values_at(path, "d.os", vec![w], "page").expect("the page's write")
}

/// What the page holds once it has taken the answer, as `applyDocumentChanges` or a rebuild does.
fn taken(mut held: Vec<OverseerNode>, update: app_api::ResolvedUpdate) -> Vec<OverseerNode> {
    match (update.changes, update.nodes) {
        (Some(changes), _) => {
            delta::apply(&mut held, changes);
            held
        }
        (None, Some(nodes)) => nodes,
        (None, None) => panic!("an answer with neither changes nor a document"),
    }
}

fn the_file(path: &str) -> Vec<OverseerNode> {
    app_api::forget_baseline();
    app_api::load_document(std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn a_page_behind_the_file_is_not_given_changes_against_the_file() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (path, held) = opened("behind");
    the_bot_appends(&path);
    let update = {
        let _showing = app_api::describing_for(Some(app_api::version_of(DOCUMENT)));
        the_page_writes(&path, &held)
    };
    let now = taken(held, update);
    assert_eq!(shown(&now), shown(&the_file(&path)), "the page does not show what the file holds");
}

#[test]
fn a_page_holding_the_file_is_given_changes() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (path, held) = opened("current");
    let update = {
        let _showing = app_api::describing_for(Some(app_api::version_of(DOCUMENT)));
        the_page_writes(&path, &held)
    };
    let changes = update.changes.clone().expect("a page holding the file is told what changed");
    assert!(changes.iter().any(|c| matches!(c, DocumentChange::Parameters { address, .. } if address == "t/Rows/[c]/qty")));
    let now = taken(held, update);
    assert_eq!(shown(&now), shown(&the_file(&path)));
}

#[test]
fn a_caller_that_names_nothing_is_answered_as_before() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (path, held) = opened("unnamed");
    assert!(the_page_writes(&path, &held).changes.is_some());
}

const GUARDED: &str = r#"tab t (mutable=true) {
    int showing (mutable="guarded") = 0
    int shown = $(showing * 10)
    button next {
        on click {
            set (path="../showing") = $(../showing + 1)
        }
    }
}
"#;

/// A viewer's own value is in the text that viewer is shown and not in the file, so the two
/// always differ while one is held - and a second press is described against the first press's
/// answer, rather than sent the whole document every time.
#[test]
fn a_viewers_second_press_is_given_changes() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("overseer_shown_guarded_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("g.os");
    std::fs::write(&file, GUARDED).unwrap();
    let path = file.to_string_lossy().to_string();
    app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let mut held = app_api::load_document(GUARDED.to_string()).expect("open");
    let mut showing = app_api::version_of(GUARDED);
    for press in 1..=2 {
        let update = {
            let _showing = app_api::describing_for(Some(showing.clone()));
            app_api::for_the_page(
                app_api::run_event_at(&path, "g.os", vec!["t".into(), "next".into()], "click".into(), "viewer", vec![], vec![])
                    .expect("the press"),
            )
        };
        assert!(update.changes.is_some(), "press {} was answered with the whole document", press);
        showing = update.version.clone();
        held = taken(held, update);
    }
    let shown = held[0].children.iter().find(|c| c.name == "shown").unwrap();
    assert_eq!(shown.parameters.get("_computed_value"), Some(&OverseerValue::Integer(20)));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), GUARDED, "the viewer's value reached the file");
}

/// The page works the version of a text out for itself when it holds the text rather than a
/// version, so the two have to agree byte for byte - `tests/a_page_names_the_text_it_shows.spec.js`
/// asserts the same value for the same text.
#[test]
fn a_version_is_the_same_on_both_sides() {
    assert_eq!(
        app_api::version_of("tab t (label=\"D\u{ed}a \u{2713}\") {\n    int n = 1\n}\n"),
        "1a551fd9d4756f61-43"
    );
}
