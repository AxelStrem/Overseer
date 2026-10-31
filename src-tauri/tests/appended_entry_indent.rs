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

use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;
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

// An indent read from a neighbour was taken as a width, its tabs counted as spaces, and pushed
// after the tabs already on the line: each entry opened twice as deep as the one before and closed
// at half that, and a document of a few hundred entries reached a gigabyte of spaces.

fn a_list(params: &str, entries: &str) -> String {
    format!(
        concat!(
            "div (hidden=true) {{\n",
            "    div Rec {{\n",
            "        int eid = 0\n",
            "        string note = \"\"\n",
            "    }}\n",
            "}}\n",
            "tab t (mutable=true) {{\n",
            "    list History (entry=<Rec>, key=\"eid\"{}) {{\n",
            "{}",
            "    }}\n",
            "}}\n",
        ),
        params, entries
    )
}

const TABBED: &str = "\t\t- {\n            - eid = 1\n\t\t}\n";
const SORTED: &str = ", sort_by=$(|x| 0 - x/eid)";

/// The text of the document after five appends, each one read from the file the last one wrote.
fn after_appends(tag: &str, text: &str, list: &str) -> String {
    let root = std::env::temp_dir().join(format!("overseer_indent_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), text).expect("write");
    for eid in 11..16 {
        let service = DocumentRoot::new(&root).expect("open the root");
        let mut fields = std::collections::HashMap::new();
        fields.insert("eid".to_string(), OverseerValue::Integer(eid));
        service.append_at("d.os", list, &fields).expect("append");
    }
    let after = std::fs::read_to_string(root.join("d.os")).expect("read");
    let _ = std::fs::remove_dir_all(&root);
    after
}

/// The blanks before each line of the list that opens or closes an entry, as they stand.
fn entry_blanks(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.trim() == "- {" || l.trim() == "}")
        .map(|l| l[..l.len() - l.trim_start().len()].to_string())
        .collect()
}

fn every_new_entry_is_at(text: &str, blanks: &str) {
    let found = entry_blanks(text);
    let at_it = found.iter().filter(|b| b.as_str() == blanks).count();
    assert!(at_it >= 10, "five new entries are not at {:?}: {:?}\n{}", blanks, found, text);
    assert!(
        found.iter().all(|b| b.len() <= 8),
        "an entry went deeper than the entries around it: {:?}\n{}",
        found,
        text
    );
}

#[test]
fn a_new_entry_keeps_a_tabbed_neighbours_indent() {
    let after = after_appends("tabbed", &a_list("", TABBED), "t/History");
    every_new_entry_is_at(&after, "\t\t");
}

#[test]
fn a_sorted_list_keeps_a_tabbed_neighbours_indent() {
    let after = after_appends("sorted", &a_list(SORTED, TABBED), "t/History");
    every_new_entry_is_at(&after, "\t\t");
}

#[test]
fn a_new_entry_takes_the_last_of_entries_laid_out_unevenly() {
    let entries = "\t\t - { - eid = 1 }\n      - {\n            - eid = 2\n   }\n\t\t    - {\n            - eid = 3\n   }\n";
    let before = a_list(SORTED, entries);
    let after = after_appends("uneven", &before, "t/History");
    every_new_entry_is_at(&after, "\t\t    ");
    assert!(after.contains(entries), "the entries already there were moved:\n{}", after);
}

#[test]
fn a_nested_list_keeps_a_tabbed_neighbours_indent() {
    // A list inside an entry is given back by splicing into the entry's own text.
    let text = concat!(
        "div (hidden=true) {\n",
        "    div Rec {\n",
        "        int eid = 0\n",
        "    }\n",
        "    div Day {\n",
        "        string day = \"\"\n",
        "        list Recs (entry=<Rec>, key=\"eid\") { }\n",
        "    }\n",
        "}\n",
        "tab t (mutable=true) {\n",
        "    list Days (entry=<Day>, key=\"day\") {\n",
        "        - {\n",
        "            - day = \"a\"\n",
        "            list Recs {\n",
        "\t\t\t\t- {\n",
        "                    - eid = 1\n",
        "\t\t\t\t}\n",
        "            }\n",
        "        }\n",
        "    }\n",
        "}\n",
    );
    let after = after_appends("nested", text, "t/Days/[a]/Recs");
    let tabbed = entry_blanks(&after).iter().filter(|b| b.as_str() == "\t\t\t\t").count();
    assert_eq!(tabbed, 12, "an entry left its neighbours' indent:\n{}", after);
    assert!(after.len() < text.len() + 5 * 60, "the document grew out of hand:\n{}", after);
}
