//! Several documents are kept at once, within a budget.
//!
//! The bug these are for: what had already been worked out lived in a single slot, so being asked
//! about a second document threw away the first. On the real documents that took a reopen of the
//! food tracker from 0.96 seconds back to 8.72 - and the bot interleaves documents inside one
//! conversation, so it almost never saw a hit and paid a full resolve nearly every request.
//!
//! Holding everything is not the answer either. A resolved `tracker_v2.os` and its graph come to
//! about 150 MB, measured with an allocator, so keeping the five documents in use would cost more
//! than the whole idle footprint the server was brought down to. Hence a budget in bytes, and the
//! cases below are mostly about what happens when it is reached.

use overseer::{app_api, document_cache};

/// The budget and the store are process-wide.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_budget<T>(mb: usize, body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    app_api::forget_dependencies();
    app_api::forget_baseline();
    document_cache::set_budget_override(Some(mb * 1024 * 1024));
    let out = body();
    document_cache::set_budget_override(None);
    app_api::forget_dependencies();
    app_api::forget_baseline();
    drop(guard);
    out
}

/// A document with `rows` list entries, each carrying a formula, so its size can be dialled.
fn document(name: &str, rows: usize) -> String {
    let mut text = format!(
        "tab {name} (label=\"T\") {{\n\
         \x20   div (hidden=true) {{\n\
         \x20       div Row (layout=\"horizontal\") {{\n\
         \x20           string kind (label=\"\") = \"\"\n\
         \x20           float size (label=\"\") = 0\n\
         \x20           float doubled (label=\"\") = $(size * 2 + 1 - 1)\n\
         \x20       }}\n\
         \x20   }}\n\
         \x20   list Rows (entry=<Row>) {{\n"
    );
    for i in 0..rows {
        text.push_str(&format!(
            "\x20       - {{\n\
             \x20           - kind = \"row number {i} of this document\"\n\
             \x20           - size = {i}\n\
             \x20       }}\n"
        ));
    }
    text.push_str(
        "\x20   }\n\
         \x20   float total (label=\"\") = $(Rows.map(|x| x/doubled).sum())\n\
         }\n",
    );
    text
}

#[test]
fn a_second_document_does_not_displace_the_first() {
    with_budget(512, || {
        let one = document("one", 20);
        let two = document("two", 20);

        app_api::load_document_with_dependencies(one.clone()).expect("one");
        app_api::load_document_with_dependencies(two.clone()).expect("two");

        assert!(
            document_cache::graph_for(&one).is_some(),
            "the first document's graph was thrown away by the second - the very bug this is for"
        );
        assert!(document_cache::graph_for(&two).is_some(), "the second's is missing too");
        assert!(document_cache::nodes_for(&one).is_some(), "the first document itself went");

        let (held, _) = document_cache::held();
        assert_eq!(held, 2, "expected both documents to be held");
    });
}

#[test]
fn several_documents_are_all_kept() {
    with_budget(512, || {
        let all: Vec<String> = (0..5).map(|i| document(&format!("d{i}"), 12)).collect();
        for text in &all {
            app_api::load_document_with_dependencies(text.clone()).expect("load");
        }
        for (i, text) in all.iter().enumerate() {
            assert!(
                document_cache::graph_for(text).is_some(),
                "document {i} was not kept"
            );
        }
    });
}

#[test]
fn the_least_recently_asked_for_is_the_one_that_goes() {
    // A budget small enough to hold some but not all. `first` is deliberately consulted after
    // `second` is stored, so the stalest entry is `second` rather than the oldest one.
    with_budget(1, || {
        let first = document("first", 30);
        let second = document("second", 30);
        let third = document("third", 30);

        app_api::load_document_with_dependencies(first.clone()).expect("first");
        app_api::load_document_with_dependencies(second.clone()).expect("second");
        // Touch the first, making the second the stalest thing held.
        let _ = document_cache::graph_for(&first);
        app_api::load_document_with_dependencies(third.clone()).expect("third");

        let (held, bytes) = document_cache::held();
        assert!(held >= 1, "the budget was not so small that nothing fits");
        assert!(
            bytes <= 1024 * 1024,
            "the store is over its budget: {bytes} bytes for {held} documents"
        );
        assert!(
            document_cache::graph_for(&third).is_some(),
            "the document just stored is the one that went, which is not least-recently-used"
        );
        assert!(
            document_cache::graph_for(&second).is_none(),
            "the stalest document survived while a fresher one was evicted"
        );
    });
}

#[test]
fn a_document_too_large_for_the_budget_is_declined_rather_than_clearing_the_rest() {
    // The rule that matters on the server, where the food tracker alone is larger than the whole
    // budget: keeping four useful documents beats evicting them for one that will not fit either.
    with_budget(1, || {
        let small = document("small", 2);
        app_api::load_document_with_dependencies(small.clone()).expect("small");
        assert!(
            document_cache::graph_for(&small).is_some(),
            "the small document should fit a megabyte"
        );

        let enormous = document("enormous", 4000);
        app_api::load_document_with_dependencies(enormous.clone()).expect("enormous");

        assert!(
            document_cache::graph_for(&enormous).is_none(),
            "a document larger than the whole budget was kept anyway"
        );
        assert!(
            document_cache::graph_for(&small).is_some(),
            "the small document was evicted for one that could never fit"
        );
    });
}

#[test]
fn nothing_is_kept_when_the_budget_is_nothing() {
    with_budget(0, || {
        let text = document("off", 10);
        app_api::load_document_with_dependencies(text.clone()).expect("load");
        assert!(document_cache::graph_for(&text).is_none());
        assert!(document_cache::nodes_for(&text).is_none());
        assert_eq!(document_cache::held(), (0, 0));
    });
}

#[test]
fn an_edit_carries_what_was_held_onto_the_new_text() {
    // An edit rewrites the document, so the text it is held against changes. What was worked out
    // still describes it, and is moved rather than dropped - otherwise every edit would cost the
    // next read a full resolve.
    with_budget(512, || {
        let before = document("edited", 15);
        let other = document("bystander", 15);
        app_api::load_document_with_dependencies(before.clone()).expect("load");
        app_api::load_document_with_dependencies(other.clone()).expect("bystander");

        let update = app_api::resolve_selective_update(
            before.clone(),
            vec!["edited/Rows/Row__1/size".to_string()],
            None,
        )
        .expect("edit");

        assert!(
            document_cache::graph_for(&update.text).is_some(),
            "the graph did not follow the document onto its new text"
        );
        assert!(
            document_cache::nodes_for(&update.text).is_some(),
            "the edited document was not kept"
        );
        assert!(
            document_cache::graph_for(&other).is_some(),
            "moving one document's entry disturbed another's"
        );
    });
}

#[test]
fn the_size_of_a_document_is_never_underestimated() {
    // The budget is only as good as this number. It is allowed to be high - that only declines to
    // keep something it could have - but an underestimate spends memory that was budgeted not to
    // be spent. Checked against the real documents rather than a fixture, since what made the
    // first attempt wrong was formula-heavy parameters.
    //
    // Under the same lock as the rest, even though it asserts nothing about the budget: loading a
    // document puts it in the shared cache, and five real documents arriving while another test
    // holds the budget at a megabyte will evict what that test just put there. It did.
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    for name in [
        "../examples/tasks/tasks.os",
        "../examples/shopping/shopping.os",
        "../examples/blood_pressure/blood_pressure.os",
        "../examples/projects/project_overseer.os",
        "../examples/diary/diary.os",
    ] {
        let Ok(source) = std::fs::read_to_string(here.join(name)) else {
            continue;
        };
        let nodes = app_api::load_document(source).expect("load");
        let guessed = document_cache::footprint(&nodes);
        // A floor rather than the true allocation, which a test cannot see without replacing the
        // allocator: every parameter needs at least its own name and a bucket to sit in.
        let params: usize = {
            fn count(nodes: &[overseer::types::OverseerNode]) -> usize {
                nodes
                    .iter()
                    .map(|n| n.parameters.len() + count(&n.children))
                    .sum()
            }
            count(&nodes)
        };
        let floor = params * 64;
        assert!(
            guessed >= floor,
            "{name}: estimated {guessed} bytes for {params} parameters, which is below what they \
             must occupy ({floor})"
        );
        checked += 1;
    }
    assert!(checked > 0, "no documents were available to check");
    // Not left behind for whatever runs next.
    app_api::forget_dependencies();
    app_api::forget_baseline();
}
