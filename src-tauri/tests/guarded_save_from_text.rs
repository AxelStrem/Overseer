//! Saving from text must still write guarded fields as the document authored them.
//!
//! `mutable="guarded"` means a field can change in the open document and never reach disk.
//! The text the caller holds comes from resolving with those changes applied - that is how
//! the view updates when you navigate a day - so saving it verbatim would persist exactly
//! what guarded exists to prevent.

use overseer::app_api::{self, GuardedRevert};
use overseer::docmgr::manager::DocumentManager;
use overseer::types::*;

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

const DOC: &str = r#"tab tracker (mutable=true) {
    div Selected (mutable=true) {
        timestamp selected_date (precision="day", mutable="guarded") = $(today())
        int counter = 1
    }
}
"#;

#[test]
fn a_navigated_guarded_field_is_written_as_authored() {
    DocumentManager::set_current_document(None);

    // The text as it stands after navigating: the guarded field holds a concrete date.
    let navigated = DOC.replace("= $(today())", "= 2026-08-04");
    assert!(navigated.contains("2026-08-04"));

    let saved = app_api::save_document_from_text(
        navigated,
        vec![GuardedRevert {
            path: "tracker/Selected/selected_date".to_string(),
            value: Some(OverseerValue::Formula("today()".to_string())),
        }],
    )
    .expect("save");

    assert!(
        !saved.contains("2026-08-04"),
        "the navigated date reached the file:\n{}",
        saved
    );
    assert!(
        saved.contains("today()"),
        "the authored formula was not restored:\n{}",
        saved
    );

    // Everything else must come through untouched.
    let reopened = app_api::load_document(saved).expect("reopen");
    assert_eq!(
        find(&reopened, "counter").and_then(|n| n.parameters.get("value")),
        Some(&OverseerValue::Integer(1)),
        "an ordinary field was disturbed by the revert"
    );
}

#[test]
fn an_override_created_only_in_session_is_not_written() {
    DocumentManager::set_current_document(None);
    let navigated = DOC.replace("= $(today())", "= 2026-08-04");

    let saved = app_api::save_document_from_text(
        navigated,
        // No value: the override existed only while the document was open.
        vec![GuardedRevert {
            path: "tracker/Selected/selected_date".to_string(),
            value: None,
        }],
    )
    .expect("save");

    assert!(
        !saved.contains("2026-08-04"),
        "a session-only override reached the file:\n{}",
        saved
    );
}
