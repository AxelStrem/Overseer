//! A document worked out for one person is handed to another, as long as none of its values read
//! what differs between the two: who is looking, or how their settings and their scheme draw it.
//! Its contexts are worked out again as the other sees them, and nothing else is. The agent loop
//! writes the project trackers as one person while the owner reads them as another, and each write
//! used to make the owner's page work the whole tracker out again.
#![cfg(feature = "server")]

use overseer::access::Asker;
use overseer::accounts::Accounts;
use overseer::server::DocumentRoot;
use overseer::types::{OverseerNode, OverseerValue};
use std::path::{Path, PathBuf};

const CARD: &str = r#"div (hidden=true) {
    div Row (layout="horizontal") {
        context theme {
            - text = $(qty > 2 ? muted : "inherit")
        }
        string id = ""
        int qty = 0
    }
}

tab card (mutable=true) {
    list Rows (entry=<Row>, key="id") {
        - {
            - id = "a"
            - qty = 1
        }
        - {
            - id = "b"
            - qty = 3
        }
    }
    int total = $(Rows.sum(|x| x/qty))
    string author = ""

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

const DARK_MUTED: &str = "#94a3b8";
const LIGHT_MUTED: &str = "#64748b";

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn write(root: &Path, name: &str, text: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// ap31 and bob, with accounts, and a card of ap31's that anyone may read and change.
fn served(test: &str, card: &str) -> (DocumentRoot, PathBuf) {
    let root = std::env::temp_dir().join(format!("overseer-shared-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    write(&root, "ap31/card.os", card);
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
    overseer::app_api::forget_baseline();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root)
        .unwrap()
        .with_accounts(|name| name == "ap31" || name == "bob");
    // The sharing and the directory are documents too, each worked out once and then held: read
    // here, so that the work-outs counted below are the card's alone.
    overseer::access::shared_by(&documents.path().join("ap31"));
    overseer::people::read(documents.path());
    (documents, root)
}

/// The card as this person sees it in this scheme, and how many times this thread has worked a
/// document out whole by the end of it.
fn open(documents: &DocumentRoot, who: &str, scheme: &str) -> (Vec<OverseerNode>, usize) {
    documents.as_asker(Asker::Person(who.into()), || {
        overseer::environment::seen_as(Some(scheme), || {
            let nodes = documents.open("ap31/card.os").unwrap();
            (nodes, overseer::resolver::times_worked_out_whole())
        })
    })
}

fn value(node: &OverseerNode) -> String {
    match node.parameters.get("_computed_value").or_else(|| node.parameters.get("value")) {
        Some(OverseerValue::String(s)) => s.clone(),
        Some(OverseerValue::Integer(n)) => n.to_string(),
        other => format!("{:?}", other),
    }
}

fn child<'a>(node: &'a OverseerNode, name: &str) -> &'a OverseerNode {
    node.children
        .iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("no {} in {}", name, node.name))
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> &'a OverseerNode {
    fn look<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
        nodes.iter().find_map(|n| {
            if n.name == name && !overseer::contexts::is_context(n) {
                Some(n)
            } else {
                look(&n.children, name)
            }
        })
    }
    look(nodes, name).unwrap_or_else(|| panic!("no {}", name))
}

/// The environment's copy of a context, at the top.
fn top<'a>(nodes: &'a [OverseerNode], name: &str) -> &'a OverseerNode {
    nodes
        .iter()
        .find(|n| overseer::contexts::is_copy(n) && n.name == name)
        .unwrap_or_else(|| panic!("no {} at the top", name))
}

/// What the row's own theme makes of its text.
fn row_text(nodes: &[OverseerNode], row: usize) -> String {
    let rows = find(nodes, "Rows");
    let entry = rows.children.iter().filter(|n| !overseer::contexts::is_context(n)).nth(row).unwrap();
    let theme = entry
        .children
        .iter()
        .find(|n| overseer::contexts::is_copy(n) && n.name == "theme")
        .expect("the row has a copy of the theme");
    value(child(theme, "text"))
}

#[test]
fn a_document_that_reads_nothing_of_who_is_looking_is_worked_out_once_for_everyone() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (documents, _) = served("once", CARD);
    let (mine, worked) = open(&documents, "ap31", "light");
    assert_eq!(value(top(&mine, "mode")), "light");
    assert_eq!(row_text(&mine, 1), LIGHT_MUTED);

    let (bobs, again) = open(&documents, "bob", "dark");
    assert_eq!(again, worked, "bob's open worked the card out again");
    assert_eq!(value(top(&bobs, "mode")), "dark");
    assert_eq!(value(child(top(&bobs, "me"), "handle")), "bob");
    assert_eq!(row_text(&bobs, 0), "inherit");
    assert_eq!(row_text(&bobs, 1), DARK_MUTED, "the row's theme is not drawn dark for bob");
    assert_eq!(value(find(&bobs, "total")), "4");

    let (mine, again) = open(&documents, "ap31", "light");
    assert_eq!(again, worked);
    assert_eq!(value(top(&mine, "mode")), "light");
    assert_eq!(value(child(top(&mine, "me"), "handle")), "ap31");
    assert_eq!(row_text(&mine, 1), LIGHT_MUTED);
}

#[test]
fn a_document_handed_over_is_the_document_worked_out_for_whoever_it_is_handed_to() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (documents, _) = served("same", CARD);
    open(&documents, "ap31", "light");
    let (handed, _) = open(&documents, "bob", "dark");
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    let (worked, _) = open(&documents, "bob", "dark");
    let json = |nodes: &[OverseerNode]| serde_json::to_string_pretty(nodes).unwrap();
    let (handed, worked) = (json(&handed), json(&worked));
    if handed != worked {
        let first = handed.lines().zip(worked.lines()).position(|(a, b)| a != b).unwrap_or(0);
        let at = |text: &str| text.lines().skip(first.saturating_sub(15)).take(30).collect::<Vec<_>>().join("\n");
        panic!("handed over:\n{}\n\nworked out:\n{}", at(&handed), at(&worked));
    }
}

#[test]
fn a_document_that_reads_the_theme_is_worked_out_for_each_way_of_seeing_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let card = CARD.replace("string author = \"\"", "string author = \"\"\n    string colour = $(theme/accent)");
    let (documents, _) = served("theme", &card);
    let (mine, worked) = open(&documents, "ap31", "light");
    assert_eq!(value(find(&mine, "colour")), "#2563eb");

    let (mine, again) = open(&documents, "ap31", "dark");
    assert_eq!(again, worked + 1, "a document that reads the theme was handed over in another scheme");
    assert_eq!(value(find(&mine, "colour")), "#3b82f6");

    // Each person's themes are looked for beside their own settings first, so a theme put there
    // is theirs alone: bob does not see the theme as ap31 does, even where the two say the same.
    let (bobs, again) = open(&documents, "bob", "light");
    assert_eq!(again, worked + 2);
    assert_eq!(value(find(&bobs, "colour")), "#2563eb");
}

#[test]
fn a_document_that_reads_who_is_looking_is_worked_out_for_each_person() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let card = CARD.replace("string author = \"\"", "string author = \"\"\n    string viewer = $(me/name)");
    let (documents, _) = served("me", &card);
    let (mine, worked) = open(&documents, "ap31", "light");
    assert_eq!(value(find(&mine, "viewer")), "Dmitrii");
    let (bobs, again) = open(&documents, "bob", "light");
    assert_eq!(again, worked + 1);
    assert_eq!(value(find(&bobs, "viewer")), "Bob");
}

#[test]
fn a_press_on_a_document_held_for_someone_else_reads_whoever_pressed() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (documents, root) = served("press", CARD);
    let press = |who: &str| {
        documents.as_asker(Asker::Person(who.into()), || {
            documents
                .command_for(
                    who,
                    Some("ap31/card.os"),
                    "run_overseer_event",
                    &serde_json::json!({ "node_path": ["card", "sign"], "event_name": "click" }),
                )
                .unwrap();
        })
    };
    let author = || {
        let text = std::fs::read_to_string(root.join("ap31/card.os")).unwrap();
        text.lines().find(|l| l.contains("string author")).unwrap().trim().to_string()
    };
    open(&documents, "ap31", "light");
    press("bob");
    assert_eq!(author(), "string author = \"bob\"");
    press("ap31");
    assert_eq!(author(), "string author = \"ap31\"");
}

#[test]
fn another_persons_write_is_caught_up_without_working_the_document_out() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (documents, root) = served("catch-up", CARD);
    let path = root.join("ap31/card.os");
    let (_, _listening) = overseer::watch::listen(&path);
    let before = std::fs::read_to_string(&path).unwrap();
    let (_, worked) = open(&documents, "ap31", "dark");

    documents.as_asker(Asker::Person("bob".into()), || {
        documents.set_at("ap31/card.os", "card/Rows/[a]/qty", OverseerValue::Integer(5)).unwrap();
    });
    assert_eq!(
        overseer::resolver::times_worked_out_whole(),
        worked,
        "bob's write worked out the card ap31 held"
    );

    let update = documents.as_asker(Asker::Person("ap31".into()), || {
        overseer::environment::seen_as(Some("dark"), || {
            documents
                .command_for(
                    "page",
                    Some("card.os"),
                    "catch_up_overseer_document",
                    &serde_json::json!({ "shown_version": overseer::app_api::version_of(&before) }),
                )
                .unwrap()
        })
    });
    assert!(update.get("changes").is_some_and(|c| !c.is_null()), "the page was sent the whole card");
    assert_eq!(
        overseer::resolver::times_worked_out_whole(),
        worked,
        "ap31's catch-up worked the card out again"
    );
    let (mine, _) = open(&documents, "ap31", "dark");
    assert_eq!(value(find(&mine, "total")), "8");
    assert_eq!(value(top(&mine, "mode")), "dark");
    assert_eq!(row_text(&mine, 0), DARK_MUTED, "the row bob changed is not drawn as ap31 sees it");
}

#[test]
fn a_document_worked_out_from_a_file_someone_may_not_read_is_not_handed_to_them() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let card = format!(
        "mount secret (source=\"secret.os/notes\", lazy=false, hidden=true, mutable=false) {{ }}\n\n{}",
        CARD
    );
    let (documents, root) = served("secret", &card);
    write(&root, "ap31/secret.os", "tab notes {\n    string word = \"hush\"\n}\n");
    let (mine, _) = open(&documents, "ap31", "light");
    assert!(format!("{:?}", mine).contains("hush"), "ap31 does not see their own notes");
    let (bobs, _) = open(&documents, "bob", "light");
    assert!(!format!("{:?}", bobs).contains("hush"), "bob was handed what ap31 may read and bob may not");
}
