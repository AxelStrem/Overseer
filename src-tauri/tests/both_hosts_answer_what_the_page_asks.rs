//! Every command the page calls exists on both sides of it.
//!
//! The page is one build and runs in two places: inside the desktop app, where `invoke` reaches
//! the Tauri commands in `main.rs`, and inside a browser, where `bridge.js` turns the same call
//! into `POST /api/<cmd>` and the dispatcher in `server.rs` answers it. Two lists, one caller,
//! and nothing connecting them.
//!
//! Which is how an Undo button shipped that worked in the browser and answered "Command
//! undo_overseer_file not found" in the app: the command was added to the dispatcher and not to
//! the handler list. Nothing failed to compile, no test failed, and the feature was simply
//! absent in one of the two places it was meant to be.
//!
//! Reading source text to check this is crude, and it is crude on purpose: the alternative is
//! generating both lists from one declaration, which is a larger change than the fault deserves.
//! What matters is that adding a command to one place and not the other stops being silent.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(repo().join(relative))
        .unwrap_or_else(|e| panic!("could not read {}: {}", relative, e))
}

/// Every `invoke('name'` the frontend makes.
fn asked_for() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for source in ["src/main.js", "src/renderer.js", "src/file-manager.js", "src/drawer.js"] {
        let text = read(source);
        let mut rest = text.as_str();
        while let Some(at) = rest.find("invoke('") {
            rest = &rest[at + "invoke('".len()..];
            if let Some(end) = rest.find('\'') {
                let name = &rest[..end];
                if name.chars().all(|c| c.is_ascii_lowercase() || c == '_') && !name.is_empty() {
                    found.insert(name.to_string());
                }
            }
        }
    }
    found
}

/// Every command the desktop app registers with Tauri.
fn the_desktop_offers() -> BTreeSet<String> {
    let text = read("src-tauri/src/main.rs");
    let start = text
        .find("generate_handler!")
        .expect("main.rs no longer registers commands the way this test reads them");
    let list = &text[start..];
    let end = list.find("])").expect("the handler list is not closed as expected");
    list[..end]
        .lines()
        .skip(1)
        .map(|line| line.trim().trim_end_matches(',').to_string())
        .filter(|name| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        })
        .collect()
}

/// Every command the server's dispatcher answers.
fn the_server_offers() -> BTreeSet<String> {
    let text = read("src-tauri/src/server.rs");
    let mut found = BTreeSet::new();
    // Arms read `"name" =>` and `"name" | "other" =>`.
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.ends_with("=> {") && !trimmed.contains("\" =>") && !trimmed.contains("\" | \"") {
            continue;
        }
        let mut rest = trimmed;
        while let Some(at) = rest.find('"') {
            rest = &rest[at + 1..];
            let Some(end) = rest.find('"') else { break };
            let name = &rest[..end];
            // The command names the page uses, rather than every string an arm happens to hold -
            // the drawer's `menu_*` among them.
            if name.contains("overseer") || name.starts_with("find_") || name.starts_with("menu_") {
                found.insert(name.to_string());
            }
            rest = &rest[end + 1..];
        }
    }
    found
}

#[test]
fn the_desktop_answers_everything_the_page_asks_for() {
    let missing: Vec<String> = asked_for()
        .difference(&the_desktop_offers())
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "the page calls these and the desktop app does not register them, so they fail at \
         runtime with \"Command not found\": {:?}",
        missing
    );
}

#[test]
fn the_server_answers_everything_the_page_asks_for() {
    let missing: Vec<String> = asked_for()
        .difference(&the_server_offers())
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "the page calls these and the server's dispatcher does not answer them: {:?}",
        missing
    );
}

#[test]
fn the_two_hosts_offer_the_same_commands() {
    // Not strictly required - either may have something the page never calls - but a difference
    // is nearly always one of them having been forgotten, and it costs nothing to look.
    let desktop = the_desktop_offers();
    let server = the_server_offers();
    let only_desktop: Vec<&String> = desktop.difference(&server).collect();
    let only_server: Vec<&String> = server.difference(&desktop).collect();
    assert!(
        only_desktop.is_empty() && only_server.is_empty(),
        "the two hosts have drifted apart.\n  only in the desktop app: {:?}\n  only in the \
         server: {:?}",
        only_desktop,
        only_server
    );
}

#[test]
fn this_test_can_still_find_the_lists() {
    // If either list is renamed or restructured, the checks above would quietly pass over
    // nothing. This fails loudly instead.
    assert!(asked_for().len() > 5, "found almost nothing the page calls: {:?}", asked_for());
    assert!(
        the_desktop_offers().len() > 5,
        "found almost nothing the desktop registers: {:?}",
        the_desktop_offers()
    );
    assert!(
        the_server_offers().len() > 5,
        "found almost nothing the server answers: {:?}",
        the_server_offers()
    );
}
