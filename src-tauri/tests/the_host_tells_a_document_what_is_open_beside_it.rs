//! `context host` tells a document what it is shown in - `host/kind`, the themes there are and the
//! one chosen - and `host/open`, the document the page has open beside it, as a drawer is. `me/admin`
//! says whether whoever is looking runs the server. All of it is there whether or not anything was
//! said, empty or false where nothing was, and a document asked with another one open beside it is
//! worked out again for it.
#![cfg(feature = "server")]

use overseer::access::Asker;
use overseer::environment::{as_admin, beside};
use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use std::path::{Path, PathBuf};

const DRAWER: &str = r#"tab drawer {
    string kind = $(host/kind)
    string theme = $(host/theme)
    string chosen_mode = $(host/chosen_mode)
    string chosen_week = $(host/chosen_week_start)
    bool accounts = $(host/accounts)
    string me_name = $(me/name)
    string written = $(me/written_name)
    string open_path = $(host/open/path)
    string open_name = $(host/open/name)
    string open_label = $(host/open/label)
    string open_can = $(host/open/can)
    bool undoable = $(host/open/undoable)
    bool admin = $(me/admin)
    string colour = $(me/colour)
    string icon = $(me/icon)
    string shape = $(me/shape)
    int keys = $(me/keys.count())
    string first_key = $(me/keys.first()/name)
    enum look (vocabulary="/host/themes") = "default"
}
"#;

const NOTES: &str = "tab notes (mutable=true) {\n    string note = \"\"\n}\n";

const JOURNAL: &str = "tab journal (label=\"My journal\") {\n    string entry = \"\"\n}\n";

const SHARING: &str = r#"div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "notes.os"
        - with = "bob"
        - can = "read"
    }
}
"#;

fn write(root: &Path, name: &str, text: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// ap31 and bob, each with the drawer, and notes of ap31's that bob may read.
fn served(test: &str) -> (DocumentRoot, PathBuf) {
    let root = std::env::temp_dir().join(format!("overseer-host-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for person in ["ap31", "bob"] {
        write(&root, &format!("{}/drawer.os", person), DRAWER);
    }
    write(&root, "ap31/notes.os", NOTES);
    write(&root, "ap31/journal.os", JOURNAL);
    write(&root, "ap31/sharing.os", SHARING);
    let documents = DocumentRoot::new(&root)
        .unwrap()
        .with_accounts(|name| name == "ap31" || name == "bob");
    (documents, root)
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes.iter().find_map(|n| {
        if n.name == name && n.node_type != "context" {
            Some(n)
        } else {
            find(&n.children, name)
        }
    })
}

fn value(nodes: &[OverseerNode], name: &str) -> OverseerValue {
    let node = find(nodes, name).unwrap_or_else(|| panic!("no {}", name));
    assert!(
        !node.parameters.contains_key(overseer::bases::ERROR),
        "{}: {:?}",
        name,
        node.parameters
    );
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("{} has no value", name))
}

fn text(nodes: &[OverseerNode], name: &str) -> String {
    match value(nodes, name) {
        OverseerValue::String(s) => s,
        other => panic!("{} is {:?}", name, other),
    }
}

fn truth(nodes: &[OverseerNode], name: &str) -> bool {
    match value(nodes, name) {
        OverseerValue::Boolean(b) => b,
        other => panic!("{} is {:?}", name, other),
    }
}

fn person(name: &str) -> Asker {
    Asker::Person(name.into())
}

#[test]
fn with_nothing_said_the_host_is_a_browser_with_nothing_open() {
    let (documents, _) = served("bare");
    let nodes = documents.as_asker(person("ap31"), || documents.open("drawer.os").unwrap());
    assert_eq!(text(&nodes, "kind"), "browser");
    assert_eq!(text(&nodes, "open_path"), "");
    assert_eq!(text(&nodes, "open_name"), "");
    assert_eq!(text(&nodes, "open_label"), "");
    assert_eq!(text(&nodes, "open_can"), "");
    assert!(!truth(&nodes, "undoable"));
    assert!(!truth(&nodes, "admin"));
    assert_eq!(text(&nodes, "icon"), "");
    assert_eq!(value(&nodes, "keys"), OverseerValue::Integer(0));
}

#[test]
fn the_themes_there_are_are_a_vocabulary_and_the_chosen_one_is_named() {
    let (documents, _) = served("themes");
    let nodes = documents.as_asker(person("ap31"), || documents.open("drawer.os").unwrap());
    let names = documents.as_asker(person("ap31"), overseer::environment::themes);
    assert!(names.contains(&"default".to_string()), "{:?}", names);
    // Nobody has chosen yet: the settings are as `DEFAULT` says, the theme that ships.
    assert_eq!(text(&nodes, "theme"), "default");
    // The list is in the tree where the page looks for a vocabulary: `/host/themes`.
    let host = nodes
        .iter()
        .find(|n| n.node_type == "context" && n.name == "host")
        .expect("the host's context at the top");
    let themes = host.children.iter().find(|n| n.name == "themes").expect("host/themes");
    let tags: Vec<String> = themes
        .children
        .iter()
        .map(|entry| text(&entry.children, "tag"))
        .collect();
    assert_eq!(tags, names);
    assert_eq!(
        themes.parameters.get("key"),
        Some(&OverseerValue::String("tag".into())),
        "an entry is known by its tag"
    );
}

#[test]
fn the_look_is_said_as_the_settings_chose_it_system_and_all() {
    let (documents, root) = served("look");
    let nodes = documents.as_asker(person("ap31"), || documents.open("drawer.os").unwrap());
    assert_eq!(text(&nodes, "chosen_mode"), "system");
    assert_eq!(text(&nodes, "chosen_week"), "system");
    write(&root, "ap31/settings.os", "context mode = \"dark\"
context week_start = \"sunday\"
");
    let nodes = documents.as_asker(person("ap31"), || documents.open("drawer.os").unwrap());
    assert_eq!(text(&nodes, "chosen_mode"), "dark");
    assert_eq!(text(&nodes, "chosen_week"), "sunday");
    // Each theme is a chip with a name a person reads.
    let host = nodes
        .iter()
        .find(|n| n.node_type == "context" && n.name == "host")
        .expect("the host's context at the top");
    let themes = host.children.iter().find(|n| n.name == "themes").expect("host/themes");
    let default = themes
        .children
        .iter()
        .find(|entry| text(&entry.children, "tag") == "default")
        .expect("the shipped theme");
    assert_eq!(text(&default.children, "name"), "Default");
}

#[test]
fn a_host_keeps_accounts_where_people_and_strangers_are_told_apart() {
    let (documents, _) = served("accounts");
    let accounts = |asker: Asker, name: &str| {
        documents.as_asker(asker, || truth(&documents.open(name).unwrap(), "accounts"))
    };
    assert!(accounts(person("ap31"), "drawer.os"));
    assert!(!accounts(Asker::Anyone, "ap31/drawer.os"));
}

#[test]
fn a_name_written_with_markup_is_plain_in_me_name_and_whole_in_written_name() {
    let (documents, root) = served("written");
    let person_named = |tag: &str, name: &str| overseer::people::Person {
        tag: tag.into(),
        name: name.into(),
        colour: None,
        icon: None,
        shape: None,
    };
    overseer::people::write(&root, &[person_named("ap31", "**Dmitrii** <icon=bot>"), person_named("bob", "")]).unwrap();
    let nodes = documents.as_asker(person("ap31"), || documents.open("drawer.os").unwrap());
    assert_eq!(text(&nodes, "me_name"), "Dmitrii");
    assert_eq!(text(&nodes, "written"), "**Dmitrii** <icon=bot>");
    // Bob wrote none: both are his username.
    let nodes = documents.as_asker(person("bob"), || documents.open("drawer.os").unwrap());
    assert_eq!(text(&nodes, "written"), "bob");
}

#[test]
fn only_an_admin_reads_me_admin_as_true() {
    let (documents, _) = served("admin");
    let admin = |asker: Asker, is: bool| {
        documents.as_asker(asker, || as_admin(is, || truth(&documents.open("drawer.os").unwrap(), "admin")))
    };
    assert!(admin(person("ap31"), true));
    assert!(!admin(person("bob"), false));
    // Straight after, the same document: what ap31 was shown is not what bob is.
    assert!(admin(person("ap31"), true));
    // Nobody signed in runs nothing, whatever was said.
    assert!(!documents.as_asker(Asker::Anyone, || as_admin(true, || {
        truth(&documents.open("ap31/drawer.os").unwrap(), "admin")
    })));
}

#[test]
fn the_document_open_beside_is_described_as_the_asker_may_use_it() {
    let (documents, _) = served("open");
    let drawn_beside = |asker: &str, open: &str| {
        documents.as_asker(person(asker), || {
            let open = documents.open_beside(open);
            beside(Some(open), || documents.open("drawer.os").unwrap())
        })
    };
    let nodes = drawn_beside("ap31", "notes.os");
    assert_eq!(text(&nodes, "open_path"), "notes.os");
    assert_eq!(text(&nodes, "open_name"), "notes");
    // Not wrapped in a labelled tab: called by its file's name.
    assert_eq!(text(&nodes, "open_label"), "notes");
    assert_eq!(text(&nodes, "open_can"), "edit");

    let nodes = drawn_beside("ap31", "journal.os");
    assert_eq!(text(&nodes, "open_name"), "journal");
    assert_eq!(text(&nodes, "open_label"), "My journal");

    // The same drawer, another document beside it: worked out again, not handed back.
    let nodes = drawn_beside("ap31", "drawer.os");
    assert_eq!(text(&nodes, "open_path"), "drawer.os");
    assert_eq!(text(&nodes, "open_name"), "drawer");

    // Their own, written out in full as an address has it, is named as they name it.
    let nodes = drawn_beside("ap31", "ap31/notes.os");
    assert_eq!(text(&nodes, "open_path"), "notes.os");
    assert_eq!(text(&nodes, "open_can"), "edit");

    let nodes = drawn_beside("bob", "ap31/notes.os");
    assert_eq!(text(&nodes, "open_path"), "ap31/notes.os");
    assert_eq!(text(&nodes, "open_can"), "read");
    let nodes = drawn_beside("bob", "ap31/missing.os");
    assert_eq!(text(&nodes, "open_can"), "");
    assert_eq!(text(&nodes, "open_label"), "missing");
}

#[test]
fn a_write_to_the_open_document_makes_it_undoable_and_undo_takes_that_back() {
    let (documents, _) = served("undo");
    let undoable = || {
        documents.as_asker(person("ap31"), || {
            let open = documents.open_beside("notes.os");
            beside(Some(open), || truth(&documents.open("drawer.os").unwrap(), "undoable"))
        })
    };
    assert!(!undoable());
    documents.as_asker(person("ap31"), || {
        documents
            .set_at("notes.os", "notes/note", OverseerValue::String("written".into()))
            .unwrap();
    });
    assert!(undoable(), "a write to the open document is there to take back");
    documents.as_asker(person("ap31"), || documents.undo("notes.os").unwrap());
    assert!(!undoable(), "and once taken back, nothing is");
}

#[test]
fn me_carries_the_chip_the_directory_draws_and_the_keys_the_server_holds() {
    let (documents, root) = served("me");
    let chosen = |tag: &str, colour: Option<&str>, icon: Option<&str>, shape: Option<&str>| overseer::people::Person {
        tag: tag.into(),
        name: tag.into(),
        colour: colour.map(String::from),
        icon: icon.map(String::from),
        shape: shape.map(String::from),
    };
    overseer::people::write(
        &root,
        &[chosen("ap31", Some("#ff8800"), Some("bot"), Some("tag")), chosen("bob", None, None, None)],
    )
    .unwrap();
    let made = chrono::DateTime::parse_from_rfc3339("2026-11-01T10:00:00Z").unwrap().to_utc();
    let key = |name: &str| overseer::environment::Key {
        name: name.into(),
        made,
        used: None,
    };
    let drawn = |asker: &str, keys: Vec<overseer::environment::Key>| {
        documents.as_asker(person(asker), || {
            overseer::environment::with_keys(keys, || documents.open("drawer.os").unwrap())
        })
    };

    let nodes = drawn("ap31", vec![key("laptop"), key("bot")]);
    assert_eq!(text(&nodes, "colour"), "#ff8800");
    assert_eq!(text(&nodes, "icon"), "bot");
    assert_eq!(text(&nodes, "shape"), "tag");
    assert_eq!(value(&nodes, "keys"), OverseerValue::Integer(2));
    assert_eq!(text(&nodes, "first_key"), "laptop");

    // A key revoked: the same drawer, worked out again rather than handed back.
    let nodes = drawn("ap31", vec![key("bot")]);
    assert_eq!(value(&nodes, "keys"), OverseerValue::Integer(1));
    assert_eq!(text(&nodes, "first_key"), "bot");

    // Bob chose nothing: his chip is the template's colour, with no icon and no shape of its own.
    let nodes = drawn("bob", Vec::new());
    assert_eq!(text(&nodes, "colour"), "#6b7280");
    assert_eq!(text(&nodes, "icon"), "");
    assert_eq!(text(&nodes, "shape"), "");
    assert_eq!(value(&nodes, "keys"), OverseerValue::Integer(0));

    // Nobody signed in holds no keys, whatever was said.
    let nodes = documents.as_asker(Asker::Anyone, || {
        overseer::environment::with_keys(vec![key("laptop")], || documents.open("ap31/drawer.os").unwrap())
    });
    assert_eq!(value(&nodes, "keys"), OverseerValue::Integer(0));
    assert_eq!(text(&nodes, "colour"), "");
}
