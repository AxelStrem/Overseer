//! The page sends an instruction, not a document.
//!
//! A page held the whole document as text and sent it back to be written over the file. Anything
//! that had written in between - the bot recording a meal, another tab, the page next door - was
//! gone, and nothing anywhere said so. The bot never had that problem: it names what to change,
//! and the change is applied to whatever the file says at the moment of applying it.
//!
//! So the page does the same now. What is checked here is that it really is the same: a write
//! made elsewhere while the page was open survives, the page is still told what to repaint, a
//! field the document keeps for the viewer still does not reach the file, and each write is one
//! step to take back.

use overseer::server::DocumentRoot;
use serde_json::json;

const DOCUMENT: &str = r#"tab day (label="Day", mutable=true) {

    timestamp showing (mutable="guarded") = $(today())
    int recorded = 0
    int doubled = $(recorded * 2)

    button record (label="record") {
        on click {
            inc (path="/day/recorded", by=1)
        }
    }

    button back (label="back") {
        on click {
            set (path="/day/showing") = $(date_add_days(../showing, -1))
        }
    }
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
    let root = std::env::temp_dir().join(format!("overseer_instr_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("day.os"), DOCUMENT).expect("write");
    overseer::viewstate::forget_all();
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("day.os")).expect("read")
}

/// A field edit the way the page makes one now: what to change and what to, and no document.
fn set(service: &DocumentRoot, session: &str, path: &[&str], value: i64) -> serde_json::Value {
    service
        .command_for(
            session,
            Some("day.os"),
            "write_overseer_value",
            &json!({ "node_path": path, "value": { "Integer": value } }),
        )
        .expect("the write was refused")
}

fn press(service: &DocumentRoot, session: &str, button: &str) -> serde_json::Value {
    service
        .command_for(
            session,
            Some("day.os"),
            "run_overseer_event",
            &json!({ "node_path": ["day", button], "event_name": "click" }),
        )
        .expect("the press was refused")
}

fn text_of(answer: &serde_json::Value) -> String {
    answer.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_string()
}

#[test]
fn a_change_reaches_the_file_without_the_document_being_sent() {
    serialised(|| {
        let root = a_root("lands");
        let service = DocumentRoot::new(&root).expect("open the root");
        set(&service, "alice", &["day", "recorded"], 5);
        assert!(on_disk(&root).contains("int recorded = 5"), "{}", on_disk(&root));
    });
}

#[test]
fn a_write_made_while_the_page_was_open_is_not_lost() {
    // The fault this was filed for. Something else writes while the page is open, and the page
    // then changes a different field. Both have to survive.
    serialised(|| {
        let root = a_root("nolose");
        let service = DocumentRoot::new(&root).expect("open the root");

        service.run_event("day.os", "day/record", "click").expect("the bot's press");
        assert!(on_disk(&root).contains("int recorded = 1"));

        set(&service, "alice", &["day", "doubled"], 99);

        let text = on_disk(&root);
        assert!(text.contains("int recorded = 1"), "the other write was thrown away:
{}", text);
    });
}

#[test]
fn the_page_is_told_what_to_repaint() {
    // It sends no document, so it has to be told what moved - otherwise it would have to ask for
    // the whole thing back, which is the cost this removes. Said against what the page was last
    // shown, so the page opens the document first, as a page does.
    serialised(|| {
        let root = a_root("repaint");
        let service = DocumentRoot::new(&root).expect("open the root");
        service.open_for("alice", "day.os").expect("open the document");

        let answer = set(&service, "alice", &["day", "recorded"], 4);
        let changes = answer.get("changes").and_then(|c| c.as_array());
        assert!(changes.is_some_and(|c| !c.is_empty()), "nothing to repaint: {}", answer);
        assert!(text_of(&answer).contains("= 4"), "the answer did not carry the new document");
    });
}

#[test]
fn a_page_that_has_not_opened_it_is_given_the_whole_document() {
    // Nothing to describe a change against, so the answer is the document itself rather than a
    // list of changes to something the caller may not have.
    serialised(|| {
        let root = a_root("cold");
        let service = DocumentRoot::new(&root).expect("open the root");
        let answer = set(&service, "alice", &["day", "recorded"], 4);
        assert!(answer.get("changes").is_some_and(|c| c.is_null()));
        assert!(
            answer.get("nodes").and_then(|n| n.as_array()).is_some_and(|n| !n.is_empty()),
            "neither changes nor a document came back"
        );
    });
}

#[test]
fn what_derives_from_the_change_is_worked_out_too() {
    // Worked out for the caller, not written down: the document says how `doubled` is arrived at,
    // and writing the number it currently comes to would replace the rule with one of its answers.
    serialised(|| {
        let root = a_root("derived");
        let service = DocumentRoot::new(&root).expect("open the root");
        let answer = set(&service, "alice", &["day", "recorded"], 6);
        assert!(
            answer.to_string().contains(r#""_computed_value":{"Integer":12}"#),
            "the caller was not told what the change came to"
        );
        assert!(
            on_disk(&root).contains("int doubled = $(recorded * 2)"),
            "the formula was replaced by one of its answers:
{}",
            on_disk(&root)
        );
    });
}

#[test]
fn a_press_writes_itself_the_same_way() {
    serialised(|| {
        let root = a_root("press");
        let service = DocumentRoot::new(&root).expect("open the root");
        press(&service, "alice", "record");
        press(&service, "alice", "record");
        assert!(on_disk(&root).contains("int recorded = 2"), "{}", on_disk(&root));
    });
}

#[test]
fn a_field_the_document_keeps_for_the_viewer_still_does_not_reach_the_file() {
    serialised(|| {
        let root = a_root("guarded");
        let service = DocumentRoot::new(&root).expect("open the root");
        let before = on_disk(&root);
        press(&service, "alice", "back");
        press(&service, "alice", "back");
        assert_eq!(on_disk(&root), before, "the viewer's day reached the file");
        assert_eq!(overseer::undo::depth(&root, "day.os"), 0, "looking left a step to take back");
    });
}

#[test]
fn two_viewers_keep_their_own_day() {
    // The page no longer holds the document, so what each viewer is looking at has to be kept
    // here. This is where sessions start doing anything for the browser.
    serialised(|| {
        let root = a_root("sessions");
        let service = DocumentRoot::new(&root).expect("open the root");
        let alice = press(&service, "alice", "back");
        press(&service, "bob", "back");
        let bob = press(&service, "bob", "back");
        assert_ne!(text_of(&alice), text_of(&bob), "both viewers were shown the same day");
    });
}

#[test]
fn every_change_is_one_step_to_take_back() {
    serialised(|| {
        let root = a_root("undo");
        let service = DocumentRoot::new(&root).expect("open the root");
        set(&service, "alice", &["day", "recorded"], 1);
        set(&service, "alice", &["day", "recorded"], 2);
        assert_eq!(overseer::undo::depth(&root, "day.os"), 2);
        service.undo("day.os").expect("undo");
        assert!(on_disk(&root).contains("int recorded = 1"), "{}", on_disk(&root));
    });
}

/// The quick way of answering a write agrees with the plain one.
///
/// A write used to parse and work the document out from scratch and then work all of it out
/// again afterwards, which on `tasks.os` was most of the second each edit took - and that second
/// is what made a 400 ms save timer able to overtake the write and undo it. So when the document
/// has already been worked out for exactly this text it is reused, and only what the change
/// reaches is worked out again, which the dependency graph can say.
///
/// Both are still there: the plain way answers whenever there is no such document to reuse, or
/// when the change moved the document's shape and the graph is describing one that no longer
/// exists. Two ways of answering the same question is a thing to be nervous about, so this
/// checks they agree - on the file, on what the caller is shown, and on what derives from the
/// change.
#[test]
fn reusing_the_worked_out_document_answers_the_same() {
    serialised(|| {
        // Nothing to reuse: the long way.
        let cold_root = a_root("cold");
        let cold = DocumentRoot::new(&cold_root).expect("open the root");
        overseer::app_api::forget_baseline();
        let cold_answer = set(&cold, "alice", &["day", "recorded"], 5);

        // Opened first, so the document for this text is there to be reused: the short way.
        let warm_root = a_root("warm");
        let warm = DocumentRoot::new(&warm_root).expect("open the root");
        warm.open_for("alice", "day.os").expect("open it as a page does");
        let warm_answer = set(&warm, "alice", &["day", "recorded"], 5);

        assert_eq!(
            on_disk(&cold_root),
            on_disk(&warm_root),
            "the two ways wrote different files"
        );
        assert_eq!(
            text_of(&cold_answer),
            text_of(&warm_answer),
            "the two ways showed the caller different documents"
        );
        assert!(
            on_disk(&warm_root).contains("int recorded = 5"),
            "the write did not land:\n{}",
            on_disk(&warm_root)
        );
        // `doubled` is $(recorded * 2), and it is the graph that has to say so on the short way.
        assert!(
            text_of(&warm_answer).contains("int doubled = 10")
                || warm_answer.to_string().contains("\"Integer\":10"),
            "what derives from the change was not worked out:\n{}",
            text_of(&warm_answer)
        );
    });
}
