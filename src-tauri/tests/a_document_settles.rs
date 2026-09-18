//! Resolving a document twice says the same thing as resolving it once.
//!
//! Formula evaluation repeats until nothing changes, up to a limit. That limit is meant to be a
//! guard against a formula that genuinely never stops moving - but the food tracker was hitting
//! it every time, so what it showed was a snapshot of an unfinished computation, and *which*
//! snapshot depended on how many times the document happened to be resolved. The two entry points
//! resolved a different number of times, so the desktop app and the bot could disagree about the
//! same day's figures.
//!
//! Two things kept it moving, and both are the same mistake in different places: a value being
//! computed that nobody could ever read.
//!
//!   - A `fallback` was worked out even for a field that states a value, where a fallback is
//!     never consulted. Two fields whose fallbacks name each other - `portions` from grams,
//!     `grams` from portions - then recomputed each other for ever, and multiplying and dividing
//!     by the same weight does not return the same bits.
//!
//!   - A failed fallback read as Null, while a failed formula everywhere else reads as
//!     "invalid formula error". So a fallback that failed became Null, whatever read it then
//!     failed in the other way, and that read back as a value: a two-step cycle that could not
//!     settle no matter how many passes it was given.
//!
//! The test is black-box on purpose. Rather than reaching for pass counters, it resolves twice
//! and compares: a document that has settled cannot change when asked again, and one that has not
//! will move. That holds whatever the limit is set to and however the passes are organised.

use overseer::app_api;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

/// Every computed value in the document, in order, as text.
fn computed(nodes: &[OverseerNode]) -> Vec<String> {
    fn walk(nodes: &[OverseerNode], trail: &mut Vec<String>, out: &mut Vec<String>) {
        for n in nodes {
            trail.push(n.name.clone());
            let mut keys: Vec<&String> = n.parameters.keys().collect();
            keys.sort();
            for k in keys {
                if !k.starts_with("_computed_") {
                    continue;
                }
                out.push(format!("{}#{} = {:?}", trail.join("/"), k, n.parameters[k]));
            }
            walk(&n.children, trail, out);
            trail.pop();
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

/// What the difference is, named, because "two vectors differ" is not a useful failure.
fn settles(source: &str) -> Result<(), String> {
    let mut nodes = app_api::load_document(source.to_string()).map_err(|e| format!("{e:?}"))?;
    let first = computed(&nodes);
    resolver::resolve_document(&mut nodes);
    let again = computed(&nodes);

    if first == again {
        return Ok(());
    }
    let moved: Vec<String> = first
        .iter()
        .zip(again.iter())
        .filter(|(a, b)| a != b)
        .take(4)
        .map(|(a, b)| format!("\n    was {a}\n    now {b}"))
        .collect();
    Err(format!(
        "{} of {} values changed when the document was resolved again:{}",
        first.iter().zip(again.iter()).filter(|(a, b)| a != b).count(),
        first.len(),
        moved.join(""),
    ))
}

/// Two fields that each fall back to the other, which is how an amount can be given either way
/// round: state the portions and the grams follow, or state the grams and the portions do.
///
/// The weight is deliberately not a round number. Multiplying and dividing by 35 returns the bits
/// it started with, so a pair built on it settles by luck even when the machinery is wrong; 33.3
/// does not, which is what the real catalog looks like and what the fix has to cope with.
const MUTUAL_FALLBACKS: &str = r#"
tab t (label="T") {
    div (hidden=true) {
        div Meal (layout="horizontal") {
            float portion_weight (hidden=true) = 33.3
            float portions (label="", fallback=$(grams / portion_weight)) = null
            float grams (label="", fallback=$(portions * portion_weight)) = null
        }
    }

    list Meals (entry=<Meal>) {
        - {
            - portions = 3
        }
        - {
            - grams = 99.9
        }
        - { }
    }
}
"#;

#[test]
fn a_pair_of_fallbacks_that_name_each_other_settles() {
    // The shape from the food tracker, reduced.
    //
    // Worth saying what this does not do: it passes against the broken resolver too. Several
    // attempts at a small document that oscillates the way the real one did came out stable, so
    // whatever tips it over needs more of the tracker than is reproduced here. It stands as a
    // check that the ordinary case keeps working, and `the_documents_in_use_settle` below is the
    // test that actually fails without the fix.
    if let Err(what) = settles(MUTUAL_FALLBACKS) {
        panic!("{what}");
    }
}

#[test]
fn a_stated_amount_is_read_and_the_other_one_follows() {
    // What the pair of fallbacks is for, and the contract the fix must not have broken: state
    // either side and the other is worked out. Asserted on the values a reader sees rather than
    // on whether a fallback was computed - an unread fallback lingering on a node is untidy, not
    // wrong, and the entries inherit one from the template whatever this does.
    let nodes = app_api::load_document(MUTUAL_FALLBACKS.to_string()).expect("load");

    fn seek<'a>(nodes: &'a [OverseerNode], want: &str) -> Vec<&'a OverseerNode> {
        let mut found = Vec::new();
        for n in nodes {
            if n.name == want {
                found.push(n);
            }
            found.extend(seek(&n.children, want));
        }
        found
    }

    let number = |n: &OverseerNode| -> Option<f64> {
        match n.parameters.get("_computed_value").or(n.parameters.get("value")) {
            Some(OverseerValue::Float(f)) => Some(*f),
            Some(OverseerValue::Integer(i)) => Some(*i as f64),
            _ => None,
        }
    };

    // Entries are in document order under the list, so the first states portions and the second
    // states grams. The template's own copy of each field comes first overall and is skipped.
    let portions: Vec<Option<f64>> = seek(&nodes, "portions").iter().map(|n| number(n)).collect();
    let grams: Vec<Option<f64>> = seek(&nodes, "grams").iter().map(|n| number(n)).collect();

    assert!(
        portions.contains(&Some(3.0)),
        "the entry that states three portions does not read three: {portions:?}"
    );
    assert!(
        grams.contains(&Some(99.9)),
        "the entry that states 99.9 grams does not read it: {grams:?}"
    );
}

#[test]
fn the_documents_in_use_settle() {
    // The ones this is really about, and the test that catches the fault: run against the
    // resolver as it was, this fails on the food tracker and the exercise log. A document that
    // does not settle shows figures that depend on how it was opened, which is the kind of wrong
    // that is never noticed.
    // Empty, and it took removing timers to get there. All three documents that never reached a
    // fixed point were the three that used one: the two task schedulers were built around a
    // timer generator and went with it, and the actions fixture held a `timestamp T = $(now())`
    // for its timers to fire against - a value that is different on every pass, which is what
    // never settling means. Kept as a list rather than deleted, so a document that stops
    // settling has somewhere to be recorded, and so the number can only go down.
    const KNOWN_UNSETTLED: &[&str] = &[];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let mut checked = 0;
    let mut unsettled = Vec::new();
    let mut still_known = Vec::new();

    fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect(&path, out);
                } else if path.extension().map(|e| e == "os").unwrap_or(false) {
                    out.push(path);
                }
            }
        }
    }

    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();

    for file in files {
        let Ok(source) = std::fs::read_to_string(&file) else { continue };
        // A document that cannot be loaded at all is another test's business.
        if app_api::load_document(source.clone()).is_err() {
            continue;
        }
        checked += 1;
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        match (settles(&source), KNOWN_UNSETTLED.contains(&name.as_str())) {
            (Err(what), false) => unsettled.push(format!("{name}: {what}")),
            (Err(_), true) => still_known.push(name),
            (Ok(()), true) => {
                panic!("`{name}` settles now - take it out of KNOWN_UNSETTLED")
            }
            (Ok(()), false) => {}
        }
    }

    assert!(checked > 0, "no documents were checked");
    assert_eq!(
        still_known.len(),
        KNOWN_UNSETTLED.len(),
        "the known list names documents that are no longer there or no longer fail",
    );
    assert!(
        unsettled.is_empty(),
        "{} of {} documents never stop changing:\n{}",
        unsettled.len(),
        checked,
        unsettled.join("\n"),
    );
}
