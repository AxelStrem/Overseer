//! `../x` written from inside a div with no name of its own.
//!
//! `..` climbs one segment of the node path, and the path does not always have a segment where
//! the document has a container: a div with no name is looked through, by the resolver and by
//! every address. So a field inside one, reading `../sibling`, climbs to the div rather than to
//! the record - and the name it wants is the div's sibling, not its child.
//!
//! It has always found it, by walking every descendant of every ancestor in turn until something
//! with that name turned up. That is expensive and it is careless: a descendant with the right
//! name may belong to something else entirely. Climbing by name instead is cheaper and says what
//! somebody writing `../` meant.
//!
//! The documents in use lean on this heavily. On tasks.os, 1,085 reads out of 50,382 arrived at
//! that search and took 18% of the whole open.

use overseer::parser::parse_document;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(hit) = find(&node.children, name) {
            return Some(hit);
        }
    }
    None
}

fn resolved(source: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let mut nodes = parse_document(source).expect("parse").1;
    resolver::resolve_document(&mut nodes);
    nodes
}

/// What this field was worked out to be.
fn value_of(nodes: &[OverseerNode], name: &str) -> String {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no node named `{}`", name));
    match node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
    {
        Some(OverseerValue::String(s)) => s.clone(),
        Some(OverseerValue::Integer(i)) => i.to_string(),
        Some(OverseerValue::Float(f)) => f.to_string(),
        Some(OverseerValue::Boolean(b)) => b.to_string(),
        other => format!("{:?}", other),
    }
}

#[test]
fn it_finds_the_sibling_of_the_wrapper() {
    // `reads_it` sits inside the unnamed row; `stated` is the row's sibling. One `..` is what
    // the document says and what a person means.
    let nodes = resolved(
        r#"tab t (label="T") {
    div record {
        int stated = 7

        div (layout="horizontal") {
            int reads_it = $(../stated)
        }
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "reads_it"), "7");
}

#[test]
fn it_climbs_past_as_many_wrappers_as_there_are() {
    // The tracker has two between a day and its Add buttons. Nothing says one is the limit.
    let nodes = resolved(
        r#"tab t (label="T") {
    div record {
        int stated = 7

        div (layout="vertical") {
            div (layout="horizontal") {
                int reads_it = $(../stated)
            }
        }
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "reads_it"), "7");
}

#[test]
fn the_nearest_one_wins() {
    // Two things called `stated`, one on the record and one on what holds the record. A `../`
    // written inside the record means the record's own, and the old search - which walked every
    // descendant of every ancestor, starting from the outermost it could - had no way to say so.
    let nodes = resolved(
        r#"tab t (label="T") {
    div holder {
        int stated = 100

        div record {
            int stated = 7

            div (layout="horizontal") {
                int reads_it = $(../stated)
            }
        }
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "reads_it"), "7");
}

#[test]
fn a_named_container_still_takes_a_hop_of_its_own() {
    // A div with a name is addressed, so `..` climbing out of it lands on the div - and one more
    // is needed to reach the record. Climbing must not paper over that by finding the name
    // anywhere above: the document said one hop, and one hop from a named div is the div.
    let nodes = resolved(
        r#"tab t (label="T") {
    div record {
        int stated = 7

        div inner {
            int stated = 3
            int reads_it = $(../stated)
        }
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "reads_it"), "3");
}

#[test]
fn a_name_that_is_nowhere_above_is_still_an_error() {
    // Climbing makes more things findable, and this must not become one of them: a mistake in
    // the document reads as a mistake.
    let nodes = resolved(
        r#"tab t (label="T") {
    div record {
        int stated = 7

        div (layout="horizontal") {
            int reads_it = $(../nothing_of_the_sort)
        }
    }
}
"#,
    );
    let said = value_of(&nodes, "reads_it");
    assert!(
        said.contains("error") || said == "None",
        "a name that is nowhere above read as `{}`",
        said
    );
}

#[test]
fn a_child_of_a_further_ancestor_beats_a_grandchild_of_a_nearer_one() {
    // Where the two readings part company, and the only case I could construct that they answer
    // differently. The nearest ancestor has no `stated` of its own but something below it does,
    // buried inside a named group that has nothing to do with the read. A further ancestor has
    // one as a plain child.
    //
    // Walking every descendant of the nearest ancestor finds the buried one and answers 99.
    // Climbing by name passes over it - it is not a child - and finds the field on the record,
    // which is what `../` is for. The old answer was not wrong by accident; it was reaching for
    // whatever had the name.
    let nodes = resolved(
        r#"tab t (label="T") {
    div record {
        int stated = 7

        div group {
            div buried {
                int stated = 99
            }

            div (layout="horizontal") {
                int reads_it = $(../stated)
            }
        }
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "reads_it"), "7");
}

#[test]
fn it_reads_a_field_that_is_itself_worked_out() {
    // The case the documents actually have: the sibling is not a stated value but a formula of
    // its own, so reading it has to give the worked-out answer rather than the formula.
    let nodes = resolved(
        r#"tab t (label="T") {
    div record {
        int stated = 7
        bool big (hidden=true) = $(stated > 3)

        div (layout="horizontal") {
            string reads_it = $(../big ? "yes" : "no")
        }
    }
}
"#,
    );
    assert_eq!(value_of(&nodes, "reads_it"), "yes");
}
