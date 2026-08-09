//! An entry added to a list must close where it opened.
//!
//! Inside a list body the opening of an entry is lined up with the entries around it, which is
//! not always the indent worked out from its depth - a new entry has no source text of its own
//! to take the layout from. The closing brace was using the computed indent regardless, so an
//! entry opened at one depth and closed at another.
//!
//! Nothing was lost - the document parses back either way - but the next save reflowed it, so
//! saving twice produced two different files. That is how it was noticed: as an unstable save
//! rather than as a broken entry.

use overseer::{actions::ActionExecutor, addressing, app_api, docmgr::manager::DocumentManager};

const DOC: &str = r#"tab t (mutable=true) {
    div (hidden=true) {
        div Row (layout="vertical") {
            string name = ""
            int qty = 0
        }
    }
    list Rows (entry=<Row>) {
        - {
            - name = "first"
            - qty = 1
        }
    }
    button add (label="Add") {
        on click {
            append (list="/t/Rows") {
                - name = "second"
                - qty = 2
            }
        }
    }
}
"#;

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The indent of each `- {` and the `}` that closes it, in order.
fn entry_braces(text: &str) -> Vec<(usize, usize)> {
    let from = text.find("list Rows").expect("the list");
    let lines: Vec<&str> = text[from..].lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() != "- {" {
            continue;
        }
        let open = indent_of(line);
        // The matching close is the next line at the same indent holding only a brace.
        for later in &lines[i + 1..] {
            if later.trim() == "}" && indent_of(later) <= open {
                out.push((open, indent_of(later)));
                break;
            }
        }
    }
    out
}

fn press_add(text: &str) -> String {
    let mut nodes = app_api::load_document(text.to_string()).expect("load");
    let path = addressing::name_path(&nodes, "t/add").expect("the button");
    ActionExecutor::execute_event(&mut nodes, &path, "click").expect("press");
    overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize")
}

#[test]
fn a_new_entry_closes_where_it_opened() {
    DocumentManager::set_current_document(None);
    let after = press_add(DOC);
    let braces = entry_braces(&after);
    assert_eq!(braces.len(), 2, "expected two entries, got {:?}", braces);
    for (open, close) in &braces {
        assert_eq!(
            open, close,
            "an entry opened at {} and closed at {}:\n{}",
            open, close, after
        );
    }
}

#[test]
fn saving_again_changes_nothing() {
    // The consequence that made this visible. A document whose entries close where they open
    // is one the serializer has nothing left to tidy.
    DocumentManager::set_current_document(None);
    let once = app_api::canonicalize_document(&press_add(DOC));
    let twice = app_api::canonicalize_document(&once);
    assert_eq!(once, twice, "the second save reflowed what the first wrote");
}

#[test]
fn an_entry_that_was_already_there_is_left_alone() {
    // The fix must not reach beyond new entries: an existing one keeps the layout it was
    // written with, which is the whole point of replaying source.
    DocumentManager::set_current_document(None);
    let after = press_add(DOC);
    assert!(
        after.contains("            - name = \"first\""),
        "the entry that was already there was reflowed:\n{}",
        after
    );
}
