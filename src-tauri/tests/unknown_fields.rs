//! An append that names a field the entry does not have must be refused.
//!
//! Accepting one silently is how a plate of khinkali became an apple. The bot was told to
//! record "the handle and one amount", wrote `handle` where the template says `food`, and the
//! server took it: the amount landed, the food did not, and the entry fell back to the
//! template's default food. It resolved to a real and entirely plausible meal, and the stray
//! field was pruned on the next write, so nothing was left pointing at what had gone wrong.
//!
//! The caller is a model. Told which names exist it fixes itself in one step; told nothing it
//! diagnoses a bug in the resolver and starts deleting records to work around it.

use overseer::server::{DocumentRoot, RequestError};
use overseer::types::OverseerValue;

const DOCUMENT: &str = r#"tab tracker (label="T", mutable=true) {
    div (hidden=true) {
        div MealRecord (layout="vertical", margin=0) {
            string food (hidden=true) = "apple"
            div (layout="horizontal", margin=0) {
                float portions = null
                float grams = null
            }
            div per_100g (layout="horizontal", margin=0) {
                float calories = 100
            }
        }
    }

    list intake (entry=<MealRecord>) { }
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

fn sandbox(tag: &str, name: &str, text: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_fields_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(root.join(name), text).expect("write");
    let documents = DocumentRoot::new(&root).expect("root");
    (Sandbox { root }, documents)
}

fn documents(tag: &str) -> (Sandbox, DocumentRoot) {
    sandbox(tag, "tracker.os", DOCUMENT)
}

fn fields(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, OverseerValue> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), OverseerValue::String(v.to_string())))
        .collect()
}

#[test]
fn a_field_the_template_does_not_have_is_refused() {
    const TAG: &str = "a_field_the_template_does_not_have_is_refused";
    let (_dir, documents) = documents(TAG);
    let outcome = documents.append_at("tracker.os", "tracker/intake",
                                      &fields(&[("handle", "khinkali"), ("portions", "2")]));
    match outcome {
        Err(RequestError::Rejected(said)) => {
            assert!(said.contains("handle"), "it did not say which name was wrong: {said}");
            assert!(said.contains("food"), "it did not say what to use instead: {said}");
        }
        Err(other) => panic!("refused, but not for the right reason: {other:?}"),
        Ok(_) => panic!("the append was accepted, which is how a meal becomes an apple"),
    }
}

#[test]
fn nothing_is_written_when_a_field_is_refused() {
    const TAG: &str = "nothing_is_written_when_a_field_is_refused";
    // Half an entry is worse than none: it resolves to something plausible and wrong.
    let (_dir, documents) = documents(TAG);
    let _ = documents.append_at("tracker.os", "tracker/intake",
                                &fields(&[("handle", "khinkali"), ("portions", "2")]));
    let view = documents.read_at("tracker.os", "tracker/intake").expect("read");
    assert!(
        view.child_addresses.is_empty(),
        "an entry was written despite the refusal: {:?}",
        view.child_addresses
    );
}

#[test]
fn the_names_the_template_does_have_are_accepted() {
    const TAG: &str = "the_names_the_template_does_have_are_accepted";
    let (_dir, documents) = documents(TAG);
    documents
        .append_at("tracker.os", "tracker/intake",
                   &fields(&[("food", "khinkali"), ("portions", "2")]))
        .expect("a correct append was refused");
}

#[test]
fn a_nested_name_is_accepted_either_way_round() {
    const TAG: &str = "a_nested_name_is_accepted_either_way_round";
    // The catalogue is written with paths; both forms name the same field.
    let (_dir, documents) = documents(TAG);
    documents
        .append_at("tracker.os", "tracker/intake",
                   &fields(&[("food", "x"), ("per_100g/calories", "217")]))
        .expect("a nested path was refused");
    documents
        .append_at("tracker.os", "tracker/intake", &fields(&[("food", "y"), ("calories", "217")]))
        .expect("a bare nested name was refused");
}

#[test]
fn a_field_inside_a_layout_wrapper_is_named_at_the_level_it_reads_at() {
    const TAG: &str = "a_field_inside_a_layout_wrapper_is_named_at_the_level_it_reads_at";
    // `portions` sits inside an unnamed div that exists only to arrange the row. Nobody
    // writing a meal would know that, and nobody should have to.
    let (_dir, documents) = documents(TAG);
    documents
        .append_at("tracker.os", "tracker/intake", &fields(&[("food", "x"), ("grams", "60")]))
        .expect("a field inside a layout wrapper was refused");
}

#[test]
fn every_appendable_list_has_a_template_to_check_against() {
    // So the check is never skipped in practice. A list with no `entry` cannot be appended to
    // at all - that is older than this check and nothing to do with it - which means there is
    // no route into a list that takes field names nobody declared.
    const TAG: &str = "every_appendable_list_has_a_template_to_check_against";
    let (_dir, documents) = sandbox(
        TAG,
        "free.os",
        "tab t (label=\"T\", mutable=true) {\n    list things { }\n}\n",
    );
    let outcome = documents.append_at("free.os", "t/things", &fields(&[("whatever", "1")]));
    assert!(outcome.is_err(), "a list with no template accepted an entry");
}
