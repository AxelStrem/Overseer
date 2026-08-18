//! Removing an entry by an address that has gone stale.
//!
//! Entries of a list with no key are addressed by position, so removing one renumbers every
//! entry after it. A caller that read the list, then removes two things, is removing the wrong
//! one the second time - and nothing about the result says so.
//!
//! What that cost: a cider was catalogued and recorded correctly, then five removals and five
//! appends chased each other through one evening's meals. What survived was a lager nobody had
//! ordered, and no cider at all.

use overseer::server::{DocumentRoot, RequestError};
use overseer::types::OverseerValue;
use std::collections::HashMap;

const DOCUMENT: &str = r#"tab tracker (label="T", mutable=true) {
    div (hidden=true) {
        div MealRecord (layout="horizontal", margin=0) {
            string food (hidden=true) = ""
            float portions = 1
        }
    }

    list intake (entry=<MealRecord>) {
        - {
            - food = "cider"
        }
        - {
            - food = "lager"
        }
        - {
            - food = "wine"
        }
    }
}
"#;

struct Sandbox {
    root: std::path::PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_stale_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(root.join("tracker.os"), DOCUMENT).expect("write");
    let documents = DocumentRoot::new(&root).expect("root");
    (Sandbox { root }, documents)
}

fn expect(food: &str) -> HashMap<String, OverseerValue> {
    HashMap::from([("food".to_string(), OverseerValue::String(food.to_string()))])
}

fn foods(documents: &DocumentRoot) -> Vec<String> {
    let view = documents.read_at("tracker.os", "tracker/intake").expect("read");
    fn look(node: &overseer::types::OverseerNode) -> Option<String> {
        for child in &node.children {
            if child.name == "food" {
                return match child.parameters.get("value") {
                    Some(OverseerValue::String(s)) => Some(s.clone()),
                    _ => None,
                };
            }
            if let Some(found) = look(child) {
                return Some(found);
            }
        }
        None
    }
    view.node.children.iter().filter_map(look).collect()
}

#[test]
fn the_addresses_really_do_shift_when_one_is_removed() {
    // The premise. Without this the rest of the file is about nothing.
    let (_dir, documents) = sandbox("shift");
    assert_eq!(foods(&documents), ["cider", "lager", "wine"]);

    documents
        .remove_at("tracker.os", "tracker/intake/MealRecord__1", &HashMap::new())
        .expect("remove");
    assert_eq!(foods(&documents), ["lager", "wine"]);

    // MealRecord__2 was the wine a moment ago. It is now nothing, and __1 is the lager.
    let view = documents.read_at("tracker.os", "tracker/intake").expect("read");
    assert_eq!(view.child_addresses.len(), 2, "{:?}", view.child_addresses);
}

#[test]
fn removing_the_wrong_entry_is_refused_when_the_caller_says_what_it_meant() {
    let (_dir, documents) = sandbox("refused");
    // Read the list once - cider, lager, wine at __1, __2, __3 - and decide to remove the
    // cider and the lager, which is what a model tidying up after itself actually does.
    // Taking the cider out slides everything down: __2 is the wine now.
    documents
        .remove_at("tracker.os", "tracker/intake/MealRecord__1", &expect("cider"))
        .expect("the first removal was correct and should have gone through");

    match documents.remove_at("tracker.os", "tracker/intake/MealRecord__2", &expect("lager")) {
        Err(RequestError::Rejected(said)) => {
            assert!(said.contains("lager"), "it did not say what was expected: {said}");
            assert!(said.contains("wine"), "it did not say what is actually there: {said}");
            assert!(said.contains("renumber"), "it did not say why: {said}");
        }
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("it removed the wine while being told it was removing the lager"),
    }
    // And nothing was taken: the wine it would have eaten is still there.
    assert_eq!(foods(&documents), ["lager", "wine"]);
}

#[test]
fn the_right_entry_still_goes() {
    let (_dir, documents) = sandbox("right");
    documents
        .remove_at("tracker.os", "tracker/intake/MealRecord__2", &expect("lager"))
        .expect("a correct removal was refused");
    assert_eq!(foods(&documents), ["cider", "wine"]);
}

#[test]
fn saying_nothing_removes_by_address_as_it_always_did() {
    // A keyed list cannot go stale, and insisting on an expectation everywhere would be noise.
    let (_dir, documents) = sandbox("bare");
    documents
        .remove_at("tracker.os", "tracker/intake/MealRecord__1", &HashMap::new())
        .expect("remove");
    assert_eq!(foods(&documents), ["lager", "wine"]);
}

#[test]
fn a_missing_field_reads_as_missing_rather_than_as_a_match() {
    let (_dir, documents) = sandbox("missing");
    let nonsense = HashMap::from([(
        "nonexistent".to_string(),
        OverseerValue::String("x".to_string()),
    )]);
    match documents.remove_at("tracker.os", "tracker/intake/MealRecord__1", &nonsense) {
        Err(RequestError::Rejected(said)) => assert!(said.contains("missing"), "{said}"),
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("a field the entry does not have was treated as matching"),
    }
}
