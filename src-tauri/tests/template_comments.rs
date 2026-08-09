//! A template's comments belong to the template, not to what it makes.
//!
//! An entry added to a list is built from the template, and was inheriting the template's own
//! commentary along with its shape - so a catalogue of forty foods ended up holding forty
//! copies of the same remark about where the defaults live. Every entry added another.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::collections::HashMap;

const COMMENT: &str = "// Per 100 g - the canonical side. Defaults live here.";

fn sandbox(tag: &str) -> (std::path::PathBuf, DocumentRoot) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/weight_tracker");
    let root = std::env::temp_dir().join(format!("overseer_comments_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for file in ["tracker_v2.os", "foods.os"] {
        std::fs::copy(source.join(file), root.join(file)).unwrap();
    }
    let documents = DocumentRoot::new(&root).unwrap();
    (root, documents)
}

fn add_food(documents: &DocumentRoot, handle: &str) {
    let mut fields = HashMap::new();
    fields.insert("handle".to_string(), OverseerValue::String(handle.into()));
    fields.insert("per_100g/calories".to_string(), OverseerValue::Float(61.0));
    documents
        .append_at("foods.os", "food_catalog/Catalog", &fields)
        .expect("the food should be addable");
}

#[test]
fn adding_a_food_does_not_copy_the_templates_commentary() {
    let (root, documents) = sandbox("one");
    let before = std::fs::read_to_string(root.join("foods.os")).unwrap();
    let occurrences = |text: &str| text.matches(COMMENT).count();

    add_food(&documents, "kiwi");

    let after = std::fs::read_to_string(root.join("foods.os")).unwrap();
    assert_eq!(
        occurrences(&after),
        occurrences(&before),
        "the new entry arrived carrying the template's comment"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn nor_does_the_tenth_food() {
    // The failure worsens with use, which is how it went unnoticed: one copy looks like a
    // formatting quirk, ten look like a broken document.
    let (root, documents) = sandbox("many");
    let before = std::fs::read_to_string(root.join("foods.os")).unwrap();
    let occurrences = |text: &str| text.matches(COMMENT).count();

    for i in 0..10 {
        add_food(&documents, &format!("food_{}", i));
    }

    let after = std::fs::read_to_string(root.join("foods.os")).unwrap();
    assert_eq!(occurrences(&after), occurrences(&before));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_template_keeps_its_own_comment() {
    // The point is not to delete the commentary - it explains the document.
    let (root, documents) = sandbox("kept");
    add_food(&documents, "kiwi");
    let after = std::fs::read_to_string(root.join("foods.os")).unwrap();
    assert!(
        after.contains(COMMENT),
        "the template lost the comment that explains it"
    );
    let _ = std::fs::remove_dir_all(&root);
}
