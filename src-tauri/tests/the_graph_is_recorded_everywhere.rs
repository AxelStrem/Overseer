//! Opening a document records what everything was worked out from, in every build.
//!
//! It used to depend on who was asking. The desktop app recorded and the server did not, because
//! recording cost about seventy per cent of an open: worth it for something that opens a document
//! once and then edits it, not worth it for a bot that opens it afresh every request.
//!
//! Two measurements changed that. With the food tracker showing three days rather than forty-three
//! recording costs a quarter of an open rather than seventy per cent - 1.31 seconds against 1.05 -
//! and it buys a second open at 0.16 seconds instead of 1.05. The overhead is proportional to the
//! work and there is far less of it; the saving is most of a resolve.
//!
//! So the two agree now, and that is the point of these: what runs on the server should not differ
//! from what was tested on a desktop.

use overseer::app_api;

/// `OVERSEER_DEPENDENCY_GRAPH` is process-wide.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_setting<T>(value: Option<&str>, body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let before = std::env::var("OVERSEER_DEPENDENCY_GRAPH").ok();
    match value {
        Some(v) => std::env::set_var("OVERSEER_DEPENDENCY_GRAPH", v),
        None => std::env::remove_var("OVERSEER_DEPENDENCY_GRAPH"),
    }
    let out = body();
    match before {
        Some(v) => std::env::set_var("OVERSEER_DEPENDENCY_GRAPH", v),
        None => std::env::remove_var("OVERSEER_DEPENDENCY_GRAPH"),
    }
    drop(guard);
    out
}

const DOCUMENT: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Row (layout="horizontal") {
            int size (label="") = 0
            int doubled (label="") = $(size * 2)
        }
    }
    list Rows (entry=<Row>) {
        - {
            - size = 3
        }
        - {
            - size = 4
        }
    }
    int total (label="") = $(/t/Rows.map(|x| x/doubled).sum())
}
"#;

fn a_graph_was_kept() -> bool {
    app_api::forget_dependencies();
    app_api::forget_baseline();
    app_api::load_document(DOCUMENT.to_string()).expect("load");
    // The graph is held against the text it was recorded for, so asking for it is asking whether
    // anything was recorded at all.
    overseer::document_cache::graph_for(DOCUMENT).is_some()
}

#[test]
fn nothing_set_means_a_graph_is_recorded() {
    // The case that matters: neither build, nor Railway, sets anything.
    assert!(
        with_setting(None, a_graph_was_kept),
        "opening a document recorded nothing, so an unchanged one will be worked out again"
    );
}

#[test]
fn it_can_still_be_turned_off() {
    // For measuring against, and for the day a graph is suspected of holding a stale answer.
    for off in ["0", "off", "no", "false", "OFF"] {
        assert!(
            !with_setting(Some(off), a_graph_was_kept),
            "`{off}` did not switch recording off"
        );
    }
}

#[test]
fn anything_else_leaves_it_on() {
    // `=1` is what the old setting was written as, and it must keep meaning "on" rather than
    // being read as an unfamiliar word and quietly ignored.
    for on in ["1", "yes", "true", ""] {
        assert!(
            with_setting(Some(on), a_graph_was_kept),
            "`{on}` switched recording off when it should not have"
        );
    }
}

#[test]
fn the_setting_is_read_the_same_way_wherever_it_is_asked() {
    // `recording_is_on` is the one place that decides, and both the desktop command and the server
    // reach a resolve through `load_document`. This is the agreement itself: were the two to drift
    // apart again, it would be by one of them not going through here.
    assert!(with_setting(None, app_api::recording_is_on));
    assert!(!with_setting(Some("0"), app_api::recording_is_on));
    assert!(with_setting(Some("1"), app_api::recording_is_on));
}
