//! A derived document the size of a tracker saves an edit as that edit and nothing else.
//!
//! Thousands of lines of list entries and instances, each written with `+` elements, unnamed
//! steps, a nested override and a list of its own, several levels deep. Saved untouched it gives
//! back its bytes; after an edit at any depth, it gives back its bytes with only that edit in them.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::server::DocumentRoot;
use overseer::types::*;

const HEAD: &str = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Row {
            string key = ""
            string note = ""
        }
        div Card {
            string name = ""
            tab (label="About") {
                div {
                    string description = ""
                }
                div Facts (x=1) {
                    string born = ""
                    div {
                        string place = ""
                    }
                }
            }
            tab (label="Rows") {
                div {
                    list Rows (entry=<Row>, key="key") {
                    }
                }
                div {
                    string links = ""
                }
            }
        }
    }
"##;

const ENTRIES: usize = 120;
const INSTANCES: usize = 40;

fn card_body(i: usize, indent: &str) -> String {
    let mut rows = String::new();
    for r in 0..3 {
        rows.push_str(&format!(
            "{i}            - {{\n{i}                - key = \"r{r}\"\n{i}            }}\n",
            i = indent,
            r = r
        ));
    }
    format!(
        "{i}+string subtitle = \"s{n}\"\n\
         {i}tab {{\n\
         {i}    div (background=\"#eee\") {{ }}\n\
         {i}}}\n\
         {i}tab {{\n\
         {i}    div {{\n\
         {i}        - Rows {{\n\
         {rows}\
         {i}        }}\n\
         {i}    }}\n\
         {i}}}\n\
         {i}- Facts (x=2) {{\n\
         {i}    - born = \"b{n}\"\n\
         {i}}}\n\
         {i}- owner = \"o{n}\"\n",
        i = indent,
        n = i,
        rows = rows
    )
}

fn document() -> String {
    let mut text = String::from(HEAD);
    text.push_str("    list Cards (entry=<Card>, key=\"name\") {\n");
    for i in 0..ENTRIES {
        text.push_str(&format!(
            "        - {{\n            - name = \"card-{}\"\n{}        }}\n",
            i,
            card_body(i, "            ")
        ));
    }
    text.push_str("    }\n    div Shelf {\n");
    for i in 0..INSTANCES {
        text.push_str(&format!(
            "        <Card> shelf{} {{\n            - name = \"shelf-{}\"\n{}        }}\n",
            i,
            i,
            card_body(ENTRIES + i, "            ")
        ));
    }
    text.push_str("    }\n}\n");
    text
}

/// Written through the server, which names entries by their keys.
fn saved_after(text: &str, path: &str, value: &str) -> String {
    let root = std::env::temp_dir().join(format!(
        "overseer_large_write_{}_{}",
        std::process::id(),
        path.replace(['/', '[', ']'], "_")
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("t.os"), text).unwrap();
    let service = DocumentRoot::new(&root).unwrap();
    service
        .set_at("t.os", path, OverseerValue::String(value.to_string()))
        .expect("the write was refused");
    std::fs::read_to_string(root.join("t.os")).unwrap()
}

/// Written as the page sends an edit.
fn saved_from_the_page(text: &str, path: &str, value: &str) -> String {
    let mut changed = std::collections::HashMap::new();
    changed.insert(path.to_string(), OverseerValue::String(value.to_string()));
    let nodes = app_api::resolve_selective(text.to_string(), vec![path.to_string()], Some(changed))
        .unwrap();
    OverseerFileHandler::serialize_nodes(&nodes).unwrap()
}

fn read_at(text: &str, address: &str) -> Option<OverseerValue> {
    let root = std::env::temp_dir().join(format!(
        "overseer_large_read_{}_{}",
        std::process::id(),
        address.replace(['/', '[', ']'], "_")
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("t.os"), text).unwrap();
    let service = DocumentRoot::new(&root).unwrap();
    let found = service
        .read_at("t.os", address)
        .unwrap_or_else(|e| panic!("nothing read at {}: {:?}", address, e));
    found.node.parameters.get("value").cloned()
}

/// The one place in `text` that `before` names, said as `after` instead.
fn once(text: &str, before: &str, after: &str) -> String {
    assert_eq!(text.matches(before).count(), 1, "not one place: {}", before);
    text.replacen(before, after, 1)
}

#[test]
fn it_is_large_and_deep() {
    let text = document();
    assert!(text.lines().count() > 3000, "only {} lines", text.lines().count());
}

#[test]
fn saved_untouched_it_is_the_same_bytes() {
    let text = document();
    let nodes = app_api::load_document(text.clone()).unwrap();
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).unwrap(), text);
}

#[test]
fn an_edit_at_each_depth_is_only_that_edit() {
    let text = document();
    let deep = "                ";
    let cases: Vec<(String, &str, String)> = vec![
        // On the entry, through its own `+` element.
        (
            "t/Cards/[card-40]/subtitle".to_string(),
            "S",
            once(&text, "+string subtitle = \"s40\"", "+string subtitle = \"S\""),
        ),
        // Inside the unnamed div the entry's first step reached.
        (
            "t/Cards/[card-41]/description".to_string(),
            "D",
            once(
                &text,
                "            - name = \"card-41\"\n            +string subtitle = \"s41\"\n            tab {\n                div (background=\"#eee\") { }\n",
                &format!(
                    "            - name = \"card-41\"\n            +string subtitle = \"s41\"\n            tab {{\n                div (background=\"#eee\") {{\n{}    - description = \"D\"\n{}}}\n",
                    deep, deep
                ),
            ),
        ),
        // Inside the nested override, below an unnamed div of the template's.
        (
            "t/Cards/[card-42]/Facts/place".to_string(),
            "P",
            once(
                &text,
                "                - born = \"b42\"\n",
                "                - born = \"b42\"\n                - place = \"P\"\n",
            ),
        ),
        // In a part of the template the entry never wrote into, beside one it did.
        (
            "t/Cards/[card-43]/links".to_string(),
            "L",
            once(
                &text,
                "                            - key = \"r2\"\n                        }\n                    }\n                }\n            }\n            - Facts (x=2) {\n                - born = \"b43\"\n",
                "                            - key = \"r2\"\n                        }\n                    }\n                }\n                - links = \"L\"\n            }\n            - Facts (x=2) {\n                - born = \"b43\"\n",
            ),
        ),
        // In an instance, the same.
        (
            "t/Shelf/shelf7/Facts/place".to_string(),
            "P",
            once(
                &text,
                &format!("                - born = \"b{}\"\n", ENTRIES + 7),
                &format!(
                    "                - born = \"b{}\"\n                - place = \"P\"\n",
                    ENTRIES + 7
                ),
            ),
        ),
    ];
    for (path, value, expected) in cases {
        let saved = saved_after(&text, &path, value);
        if saved != expected {
            let first = saved
                .lines()
                .zip(expected.lines())
                .position(|(a, b)| a != b)
                .unwrap_or(0);
            let from = first.saturating_sub(5);
            panic!(
                "after an edit to {}, saved differs from line {}:\n--- saved\n{}\n--- expected\n{}",
                path,
                first + 1,
                saved.lines().skip(from).take(25).collect::<Vec<_>>().join("\n"),
                expected.lines().skip(from).take(25).collect::<Vec<_>>().join("\n"),
            );
        }
        assert_eq!(
            read_at(&saved, &path),
            Some(OverseerValue::String(value.to_string())),
            "{} did not read back",
            path
        );
    }
}

#[test]
fn an_edit_in_a_list_inside_a_step_is_only_that_edit() {
    let text = document();
    let path = "t/Cards/[card-44]/Rows/[r1]/note";
    let saved = saved_after(&text, path, "N");
    let marker = "- name = \"card-44\"";
    let at = text.find(marker).unwrap();
    let row = text[at..].find("- key = \"r1\"\n").unwrap() + at;
    let line_end = row + "- key = \"r1\"\n".len();
    let expected = format!(
        "{}                            - note = \"N\"\n{}",
        &text[..line_end],
        &text[line_end..]
    );
    if saved != expected {
        let first = saved
            .lines()
            .zip(expected.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        let from = first.saturating_sub(5);
        panic!(
            "saved differs from line {}:\n--- saved\n{}\n--- expected\n{}",
            first + 1,
            saved.lines().skip(from).take(25).collect::<Vec<_>>().join("\n"),
            expected.lines().skip(from).take(25).collect::<Vec<_>>().join("\n"),
        );
    }
    assert_eq!(read_at(&saved, path), Some(OverseerValue::String("N".to_string())));
}

fn served(tag: &str, text: &str) -> (std::path::PathBuf, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_large_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("t.os"), text).unwrap();
    let service = DocumentRoot::new(&root).unwrap();
    (root, service)
}

#[test]
fn an_entry_added_to_a_list_inside_a_step_goes_after_the_last() {
    let text = document();
    let (root, service) = served("append", &text);
    let mut fields = std::collections::HashMap::new();
    fields.insert("key".to_string(), OverseerValue::String("r3".to_string()));
    service.append_at("t.os", "t/Cards/[card-50]/Rows", &fields).expect("the append was refused");
    let saved = std::fs::read_to_string(root.join("t.os")).unwrap();
    let at = text.find("- name = \"card-50\"").unwrap();
    let last = text[at..].find("- key = \"r2\"\n").unwrap() + at;
    let end = last + "- key = \"r2\"\n                        }\n".len();
    let expected = format!(
        "{}                        - {{\n                            - key = \"r3\"\n                        }}\n{}",
        &text[..end],
        &text[end..]
    );
    assert_eq!(saved, expected);
}

#[test]
fn an_entry_taken_out_of_a_list_inside_a_step_is_gone() {
    let text = document();
    let (root, service) = served("remove", &text);
    service
        .remove_at("t.os", "t/Cards/[card-51]/Rows/[r1]", &std::collections::HashMap::new())
        .expect("the removal was refused");
    let saved = std::fs::read_to_string(root.join("t.os")).unwrap();
    let at = text.find("- name = \"card-51\"").unwrap();
    let row = text[at..].find("                        - {\n                            - key = \"r1\"\n                        }\n").unwrap() + at;
    let gone = "                        - {\n                            - key = \"r1\"\n                        }\n".len();
    let expected = format!("{}{}", &text[..row], &text[row + gone..]);
    assert_eq!(saved, expected);
}

#[test]
fn an_edit_from_the_page_is_only_that_edit() {
    let text = document();
    let born = format!("                - born = \"b{}\"\n", ENTRIES + 9);
    let saved = saved_from_the_page(&text, "t/Shelf/shelf9/Facts/place", "P");
    assert_eq!(
        saved,
        once(&text, &born, &format!("{}                - place = \"P\"\n", born))
    );
}
