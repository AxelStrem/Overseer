//! Text the parser cannot read is reported where it is, with its line.
//!
//! The parser recovers from what it cannot read - a top-level line is passed over, and inside a
//! block it moves on until something reads again - so that one stray line does not lose a whole
//! document. It used to do so without a word: a document with an unterminated string and a
//! missing brace opened as a few empty fields, and nothing said why. Now each passage it passes
//! over is left in `_parse_error` on the node nearest to it - the one it follows on its line,
//! else the next one, else the block it is in - and the page draws it there.

use overseer::app_api;
use overseer::types::*;
use std::path::Path;

const PARSE_ERROR: &str = "_parse_error";

fn errors_in(nodes: &[OverseerNode], out: &mut Vec<(String, String)>) {
    for node in nodes {
        if let Some(OverseerValue::String(why)) = node.parameters.get(PARSE_ERROR) {
            out.push((node.name.clone(), why.clone()));
        }
        errors_in(&node.children, out);
    }
}

fn errors_of(text: &str) -> Vec<(String, String)> {
    let nodes = app_api::load_document(text.to_string()).expect("a document that recovers opens");
    let mut out = Vec::new();
    errors_in(&nodes, &mut out);
    out
}

#[test]
fn an_unterminated_string_and_a_missing_brace_are_both_reported() {
    let errors = errors_of("div Page {\n    string name = \"A page\n    int x = \n");
    let said = |name: &str| {
        errors
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, why)| why.clone())
            .unwrap_or_else(|| panic!("nothing said on {name}: {errors:?}"))
    };
    let page = said("Page");
    assert!(page.contains("line 1") && page.contains("never closed"), "{page}");
    let name = said("name");
    assert!(name.contains("line 2") && name.contains("closing quote"), "{name}");
    let x = said("x");
    assert!(x.contains("line 3"), "{x}");
}

#[test]
fn a_line_that_cannot_be_read_inside_a_block_is_reported() {
    let errors = errors_of("div Page {\n    string a = \"ok\"\n    ??? junk here\n    int b = 2\n}\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let (_, why) = &errors[0];
    assert!(why.contains("line 3") && why.contains("??? junk here"), "{why}");
}

#[test]
fn a_line_that_cannot_be_read_at_the_top_is_reported() {
    let errors = errors_of("int a = 1\n??? junk\nint b = 2\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let (name, why) = &errors[0];
    assert_eq!(name, "b", "said on the line that follows it");
    assert!(why.contains("line 2") && why.contains("??? junk"), "{why}");
}

#[test]
fn what_cannot_be_read_stays_in_the_file() {
    let text = "div Page {\n    string a = \"ok\"\n    ??? junk here\n    int b = 2\n}\n";
    assert_eq!(app_api::canonicalize_document(text), text);
}

fn os_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            os_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "os") {
            out.push(path);
        }
    }
}

#[test]
fn the_example_documents_read_whole() {
    let mut files = Vec::new();
    os_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples"), &mut files);
    assert!(!files.is_empty());
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let (_, nodes) = overseer::parser::parse_document(&text).unwrap();
        let mut errors = Vec::new();
        errors_in(&nodes, &mut errors);
        assert!(errors.is_empty(), "{}: {errors:?}", file.display());
    }
}
