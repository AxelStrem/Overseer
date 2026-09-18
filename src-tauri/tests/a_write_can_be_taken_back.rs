//! Undo, which is smaller than it sounds because a document is text.
//!
//! Nothing here reverses an operation. What each write is taken back by is the text that was
//! there before it, written back - so setting a field, appending an entry, removing one and
//! moving one between lists all cost exactly the same, and a change of shape costs nothing
//! extra. That is the whole reason this is a hundred lines rather than a subsystem.
//!
//! Two properties decide whether it can be trusted. Undoing twice has to walk back two writes
//! rather than swap between the same two states, which means the undo write must not record a
//! step of its own. And the history has to survive the process, because the container restarts
//! on every deployment and a history that does not survive that is one nobody can rely on -
//! hence files beside the documents rather than a buffer in memory.
//!
//! The snapshots end in `.undo` deliberately: `Service::list` collects `*.os` and would
//! otherwise offer them as documents, and the backup stages `*.os`, `*.md` and the journal, so
//! they are never committed.

use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;

/// A directory per test, named after it, so tests cannot tread on one another's history.
fn a_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_undo_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(
        root.join("notes.os"),
        "tab notes (label=\"Notes\", mutable=true) {\n    int n = 1\n}\n",
    )
    .expect("write the document");
    root
}

fn service(root: &std::path::Path) -> DocumentRoot {
    DocumentRoot::new(root).expect("open the root")
}

fn says(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("notes.os")).expect("read")
}

fn set_to(service: &DocumentRoot, n: i64) {
    service.set_at("notes.os", "notes/n", OverseerValue::Integer(n)).expect("set");
}

#[test]
fn the_last_write_can_be_taken_back() {
    let root = a_root("the_last_write_can");
    let service = service(&root);
    let before = says(&root);

    set_to(&service, 2);
    assert!(says(&root).contains("= 2"), "the write did not land");

    service.undo("notes.os").expect("undo");
    assert_eq!(says(&root), before, "undo did not restore what was there");
}

#[test]
fn undoing_twice_walks_back_two_writes() {
    // The property that decides whether this is an undo or a toggle. The undo write must not
    // record a step of its own, or the state it just took back becomes the newest step and the
    // history never moves.
    let root = a_root("twice");
    let service = service(&root);
    let first = says(&root);

    set_to(&service, 2);
    let second = says(&root);
    set_to(&service, 3);

    service.undo("notes.os").expect("undo once");
    assert_eq!(says(&root), second, "the first undo went somewhere unexpected");

    service.undo("notes.os").expect("undo twice");
    assert_eq!(says(&root), first, "the second undo did not go back further");
}

#[test]
fn undoing_past_the_beginning_is_refused_rather_than_guessed_at() {
    let root = a_root("undoing_past_the_b");
    let service = service(&root);
    set_to(&service, 2);

    service.undo("notes.os").expect("undo");
    let refused = service.undo("notes.os");
    assert!(refused.is_err(), "undoing with no history should refuse");
}

#[test]
fn an_append_is_taken_back_like_anything_else() {
    // The case that would be hard if undo reversed operations: an entry appears, and taking it
    // back means the addresses after it move again. As text it is the same work as a field.
    let root = a_root("append");
    std::fs::write(
        root.join("list.os"),
        "tab log (label=\"Log\", mutable=true) {\n    \
         list Rows (entry=<Row>, key=\"id\") {\n        - { - id = \"a\" }\n    }\n    \
         div (hidden=true) {\n        div Row (layout=\"horizontal\") {\n            \
         string id = \"\"\n        }\n    }\n}\n",
    )
    .expect("write");
    let service = service(&root);
    let before = std::fs::read_to_string(root.join("list.os")).expect("read");

    let mut fields = std::collections::HashMap::new();
    fields.insert("id".to_string(), OverseerValue::String("b".into()));
    service.append_at("list.os", "log/Rows", &fields).expect("append");
    let after = std::fs::read_to_string(root.join("list.os")).expect("read");
    assert!(after.contains("\"b\""), "the append did not land");

    service.undo("list.os").expect("undo");
    assert_eq!(
        std::fs::read_to_string(root.join("list.os")).expect("read"),
        before,
        "an appended entry did not come out again"
    );
}

#[test]
fn one_document_is_not_undone_by_another() {
    let root = a_root("one_document_is_no");
    std::fs::write(
        root.join("other.os"),
        "tab other (label=\"Other\", mutable=true) {\n    int n = 10\n}\n",
    )
    .expect("write");
    let service = service(&root);

    set_to(&service, 2);
    service
        .set_at("other.os", "other/n", OverseerValue::Integer(20))
        .expect("set");

    service.undo("notes.os").expect("undo notes");
    assert!(says(&root).contains("= 1"), "notes was not taken back");
    assert!(
        std::fs::read_to_string(root.join("other.os")).unwrap().contains("= 20"),
        "undoing one document disturbed another"
    );
}

#[test]
fn the_history_is_not_offered_as_documents() {
    // The snapshots live beside the documents. If they were named `.os` they would be listed,
    // served, and backed up into git.
    let root = a_root("listing");
    let service = service(&root);
    set_to(&service, 2);
    set_to(&service, 3);

    let listed = service.list();
    assert_eq!(listed, vec!["notes.os".to_string()], "the history leaked into the document list");
}

#[test]
fn the_history_outlives_the_process_that_made_it() {
    // Kept on disk for this: the server restarts on every deployment, and an undo that does not
    // survive a restart is one nobody can rely on.
    let root = a_root("restart");
    let before = {
        let service = service(&root);
        let before = says(&root);
        set_to(&service, 2);
        before
    };

    // A second service over the same directory is what a restart looks like.
    let restarted = service(&root);
    restarted.undo("notes.os").expect("undo after a restart");
    assert_eq!(says(&root), before);
}

#[test]
fn the_history_is_bounded() {
    // Otherwise a document written to every few minutes fills the volume. What sits behind the
    // bound is the git backup, which commits every fifteen minutes.
    let root = a_root("bounded");
    let service = service(&root);
    for n in 2..40 {
        set_to(&service, n);
    }
    assert!(
        overseer::undo::depth(&root, "notes.os") <= 20,
        "the history grew without limit: {} steps",
        overseer::undo::depth(&root, "notes.os")
    );
}
