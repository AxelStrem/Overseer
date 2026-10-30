//! A document stores a person by their username and shows them by their display name, read
//! from the server's `people.os`: `$(me/name)` is whoever is looking, and a vocabulary mounted
//! from the directory draws everyone else. A new name is seen everywhere, and no document is
//! written for it.
#![cfg(feature = "server")]

use overseer::access::Asker;
use overseer::accounts::Accounts;
use overseer::server::{DocumentRoot, RequestError};
use overseer::types::{OverseerNode, OverseerValue};
use std::path::{Path, PathBuf};

const CARD: &str = r#"mount people (source="../people.os/People", lazy=false, hidden=true, mutable=false) { }

tab card (mutable=true) {
    string viewer = $(me/name)
    string handle = $(me/handle)
    enum author (vocabulary="/people/People") = "ap31"
    string written_by = $(/people/People.find(author)/name)

    button sign {
        on click {
            set (path="../author") = $(me/handle)
        }
    }
}
"#;

const SHARING: &str = r#"div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "card.os"
        - with = "everyone"
        - can = "edit"
    }
}
"#;

fn write(root: &Path, name: &str, text: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// ap31 and bob, with accounts, and a card of ap31's that anyone may read and change.
fn served(test: &str) -> (DocumentRoot, Accounts, PathBuf) {
    let root = std::env::temp_dir().join(format!("overseer-people-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    write(&root, "ap31/card.os", CARD);
    write(&root, "ap31/sharing.os", SHARING);
    std::fs::create_dir_all(root.join("bob")).unwrap();
    let accounts = Accounts::open(root.join("accounts.json"))
        .and_then(|a| a.with_directory(&root))
        .unwrap();
    let now = chrono::Utc::now();
    for (username, name) in [("ap31", "Dmitrii"), ("bob", "Bob")] {
        let invite = accounts.invite(None, false, now).unwrap();
        accounts.join(&invite, username, name, "a long password", now).unwrap();
    }
    let documents = DocumentRoot::new(&root)
        .unwrap()
        .with_accounts(|name| name == "ap31" || name == "bob");
    (documents, accounts, root)
}

fn person(name: &str) -> Asker {
    Asker::Person(name.into())
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes
        .iter()
        .find_map(|n| if n.name == name { Some(n) } else { find(&n.children, name) })
}

fn text(nodes: &[OverseerNode], name: &str) -> String {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no {}", name));
    match node
        .parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
    {
        Some(OverseerValue::String(s)) => s.clone(),
        other => panic!("{} is {:?}", name, other),
    }
}

#[test]
fn the_directory_is_written_from_the_accounts() {
    let (_, _, root) = served("directory");
    let directory = std::fs::read_to_string(root.join("people.os")).unwrap();
    assert!(directory.contains("- tag = \"ap31\"\r\n        - name = \"Dmitrii\""), "{}", directory);
    assert!(directory.contains("- tag = \"bob\""));
    assert_eq!(
        overseer::people::read(&root),
        vec![("ap31".to_string(), "Dmitrii".to_string()), ("bob".to_string(), "Bob".to_string())]
    );
}

#[test]
fn me_is_whoever_is_looking_and_nobody_for_a_stranger() {
    let (documents, _, _) = served("me");
    let seen = |asker: Asker| {
        documents.as_asker(asker, || {
            let nodes = documents.open("ap31/card.os").unwrap();
            (text(&nodes, "viewer"), text(&nodes, "handle"))
        })
    };
    assert_eq!(seen(person("ap31")), ("Dmitrii".to_string(), "ap31".to_string()));
    // The same document, straight after: what ap31 was shown is not what bob is.
    assert_eq!(seen(person("bob")), ("Bob".to_string(), "bob".to_string()));
    assert_eq!(seen(person("ap31")), ("Dmitrii".to_string(), "ap31".to_string()));
    assert_eq!(seen(Asker::Stranger), (String::new(), String::new()));
    assert_eq!(seen(Asker::Anyone), (String::new(), String::new()));
}

#[test]
fn a_new_name_is_seen_without_writing_the_document() {
    let (documents, accounts, root) = served("rename");
    let card = root.join("ap31/card.os");
    let before = std::fs::read(&card).unwrap();
    let read = || {
        documents.as_asker(person("bob"), || {
            let nodes = documents.open("ap31/card.os").unwrap();
            (text(&nodes, "written_by"), text(&nodes, "author"))
        })
    };
    assert_eq!(read(), ("Dmitrii".to_string(), "ap31".to_string()));

    // What `POST /api/me` does.
    accounts
        .change_own("ap31", Some("Dima"), None, None, chrono::Utc::now())
        .unwrap();
    assert!(std::fs::read_to_string(root.join("people.os")).unwrap().contains("\"Dima\""));
    assert_eq!(read(), ("Dima".to_string(), "ap31".to_string()));
    documents.as_asker(person("ap31"), || {
        assert_eq!(text(&documents.open("card.os").unwrap(), "viewer"), "Dima");
    });
    assert_eq!(std::fs::read(&card).unwrap(), before, "the document is not written");
}

#[test]
fn a_styled_name_and_a_chosen_colour_reach_the_directory() {
    let (documents, accounts, root) = served("styled");
    let styled = "<color=#e11d48 | D><color=#2563eb | ima> <icon=bot>";
    accounts
        .change_own("ap31", Some(styled), Some("#0F766E"), None, chrono::Utc::now())
        .unwrap();
    let directory = std::fs::read_to_string(root.join("people.os")).unwrap();
    assert!(directory.contains("- colour = \"#0f766e\""), "{}", directory);
    assert_eq!(directory.matches("- colour = ").count(), 1, "bob chose none: {}", directory);
    documents.as_asker(person("bob"), || {
        // Read through the vocabulary as written, for the page to draw...
        assert_eq!(text(&documents.open("ap31/card.os").unwrap(), "written_by"), styled);
    });
    documents.as_asker(person("ap31"), || {
        // ...and as plain text where a formula puts it in a string.
        assert_eq!(text(&documents.open("card.os").unwrap(), "viewer"), "Dima");
    });

    // An empty colour is the default again.
    accounts
        .change_own("ap31", None, Some(""), None, chrono::Utc::now())
        .unwrap();
    let directory = std::fs::read_to_string(root.join("people.os")).unwrap();
    assert!(!directory.contains("- colour = "), "{}", directory);
}

#[test]
fn a_press_stores_the_username() {
    let (documents, _, root) = served("press");
    documents.as_asker(person("bob"), || {
        documents
            .command_for(
                "bob",
                Some("ap31/card.os"),
                "run_overseer_event",
                &serde_json::json!({ "node_path": ["card", "sign"], "event_name": "click" }),
            )
            .unwrap();
    });
    let card = std::fs::read_to_string(root.join("ap31/card.os")).unwrap();
    assert!(card.contains("enum author (vocabulary=\"/people/People\") = \"bob\""), "{}", card);
}

#[test]
fn the_directory_is_read_by_anyone_and_changed_by_nobody() {
    let (_, _, root) = served("guarded");
    let directory = root.join("people.os").canonicalize().unwrap();
    for asker in [person("bob"), Asker::Stranger, Asker::Anyone] {
        overseer::access::within(asker.clone(), &root.canonicalize().unwrap(), || {
            assert!(overseer::access::may(&directory, overseer::access::Need::Read), "{:?} reads", asker);
            assert!(!overseer::access::may(&directory, overseer::access::Need::Edit), "{:?} edits", asker);
        });
    }
}

#[test]
fn each_person_keeps_their_own_settings() {
    let (documents, _, root) = served("settings");
    documents.as_asker(person("bob"), || {
        documents
            .command_for("", None, "settings_choose", &serde_json::json!({ "mode": "dark" }))
            .unwrap();
    });
    let bobs = std::fs::read_to_string(root.join("bob/settings.os")).unwrap();
    assert!(bobs.contains("context mode = \"dark\""), "{}", bobs);
    assert!(!root.join("ap31/settings.os").exists());
    assert!(!root.join("settings.os").exists());
    documents.as_asker(Asker::Stranger, || {
        assert_eq!(
            documents
                .command_for("", None, "settings_choose", &serde_json::json!({ "mode": "dark" }))
                .err(),
            Some(RequestError::SignIn)
        );
    });
}
