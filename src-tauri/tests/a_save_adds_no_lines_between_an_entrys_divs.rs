//! A save adds no lines between an entry's divs.
//!
//! A project item keeps its comments in `div Talk` and its loop records in `div Agents`, written
//! one straight after the other. A write anywhere else in the document put an indented blank line
//! between the two in every item that had both, and each later save grew that gap again, so a
//! document stopped giving back its bytes on its first save.

use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;

const DOCUMENT: &str = "tab project (label=\"P\", mutable=true) {\r
    div (hidden=true) {\r
        div Comment {\r
            string body = \"\"\r
        }\r
        div Run {\r
            string kind = \"\"\r
        }\r
        div Item {\r
            string handle = \"\"\r
            string title = \"\"\r
\r
            div Talk (foldable=true) {\r
                list Comments (entry=<Comment>) { }\r
            }\r
            div Agents (foldable=true) {\r
                list Work (entry=<Run>) { }\r
            }\r
\r
            int kids (hidden=true) = 0\r
        }\r
    }\r
\r
    list Items (entry=<Item>, key=\"handle\") {\r
        - {\r
            - handle = \"one\"\r
            - title = \"One\"\r
            div Talk {\r
                list Comments {\r
                    - {\r
                        - body = \"hello\"\r
                    }\r
                }\r
            }\r
            div Agents {\r
                list Work {\r
                    - {\r
                        - kind = \"triage\"\r
                    }\r
                }\r
            }\r
        }\r
        - {\r
            - handle = \"two\"\r
            - title = \"Two\"\r
        }\r
    }\r
}\r
";

fn a_root(tag: &str, text: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_blankgrow_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("p.os"), text).expect("write");
    root
}

fn on_disk(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("p.os")).expect("read")
}

fn retitle(root: &std::path::Path, title: &str) {
    let service = DocumentRoot::new(root).expect("open the root");
    service
        .set_at("p.os", "project/Items/[two]/title", OverseerValue::String(title.to_string()))
        .expect("the write was refused");
    overseer::app_api::forget_baseline();
}

#[test]
fn a_write_elsewhere_leaves_the_divs_as_they_were() {
    let root = a_root("once", DOCUMENT);
    retitle(&root, "Second");
    let expected = DOCUMENT.replace("- title = \"Two\"", "- title = \"Second\"");
    assert_eq!(on_disk(&root), expected);
}

#[test]
fn saves_after_it_change_nothing_more() {
    let root = a_root("again", DOCUMENT);
    retitle(&root, "A");
    retitle(&root, "B");
    retitle(&root, "Two");
    assert_eq!(on_disk(&root), DOCUMENT);
}

#[test]
fn a_line_holding_only_indentation_does_not_grow() {
    let start = DOCUMENT.replace(
        "            }\r\n            div Agents {",
        "            }\r\n            \r\n            div Agents {",
    );
    let root = a_root("indented", &start);
    retitle(&root, "A");
    let first = on_disk(&root);
    retitle(&root, "Two");
    let second = on_disk(&root);
    assert_eq!(first.replace("- title = \"A\"", "- title = \"Two\""), second);
    assert_eq!(second, start);
}
