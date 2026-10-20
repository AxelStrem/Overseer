//! With accounts, the root holds a folder per person, and what is in it is theirs: others reach
//! it only as its owner's `sharing.os` says, and that is asked of every file a request reaches -
//! the document named, what it mounts, the bases its templates come from.
//!
//! The answers over HTTP are in `strangers_are_asked_to_sign_in`; this is the rule itself.

use overseer::access::Asker;
use overseer::server::{DocumentRoot, RequestError};
use overseer::types::{OverseerNode, OverseerValue};
use std::path::{Path, PathBuf};

const SHARING: &str = r#"div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "foods.os"
        - with = "bob"
    }
    - {
        - document = "notes.os"
        - with = "bob"
        - can = "edit"
    }
    - {
        - document = "recipes.os"
        - with = "everyone"
    }
    - {
        - document = "guestbook.os"
        - with = "everyone"
        - can = "edit"
    }
    - {
        - document = "sharing.os"
        - with = "everyone"
    }
}
"#;

const BOBS_SHARING: &str = r#"div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "peeks.os"
        - with = "ap31"
    }
    - {
        - document = "card.os"
        - with = "ap31"
    }
}
"#;

fn write(root: &Path, name: &str, text: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// Two people's folders, and a document of the server's own at the root.
fn people(test: &str) -> (DocumentRoot, PathBuf) {
    let root = std::env::temp_dir().join(format!("overseer-access-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    write(&root, "journal.os", "string said = \"the server's\"\n");
    write(&root, "ap31/sharing.os", SHARING);
    write(&root, "ap31/weight.os", "int kg = 70\n");
    write(&root, "ap31/foods.os", "div Catalog {\n    string apple = \"Apple\"\n}\n");
    write(&root, "ap31/notes.os", "tab notes (mutable=true) {\n    string note = \"hello\"\n}\n");
    write(&root, "ap31/recipes.os", concat!(
        "tab recipes (mutable=true) {\n",
        "    string soup = \"lentil\"\n",
        "    button change {\n",
        "        on click {\n",
        "            set (path=\"../soup\") = \"tomato\"\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));
    write(&root, "ap31/guestbook.os", "tab guestbook (mutable=true) {\n    string last = \"\"\n}\n");
    write(&root, "ap31/private.os", "div Diary {\n    string entry = \"a secret\"\n}\n");
    write(&root, "ap31/templates.os", "div Card {\n    string colour = \"red\"\n}\n");
    write(&root, "ap31/projects/plan.os", "string step = \"first\"\n");
    write(&root, "bob/sharing.os", BOBS_SHARING);
    write(&root, "bob/weight.os", concat!(
        "int kg = 80\n",
        "mount FOODS (source=\"../ap31/foods.os/Catalog\", lazy=false, hidden=true) { }\n",
    ));
    write(&root, "bob/peeks.os", concat!(
        "string mine = \"bob's\"\n",
        "mount PRIVATE (source=\"../ap31/private.os/Diary\", lazy=false) { }\n",
    ));
    write(&root, "bob/card.os", "<../ap31/templates.os/Card> card { }\n");
    let documents = DocumentRoot::new(&root)
        .unwrap()
        .with_accounts(|name| name == "ap31" || name == "bob");
    (documents, root)
}

fn person(name: &str) -> Asker {
    Asker::Person(name.into())
}

fn find<'a>(nodes: &'a [OverseerNode], name: &str) -> Option<&'a OverseerNode> {
    nodes
        .iter()
        .find_map(|n| if n.name == name { Some(n) } else { find(&n.children, name) })
}

fn value(nodes: &[OverseerNode], name: &str) -> Option<OverseerValue> {
    let node = find(nodes, name)?;
    node.parameters
        .get("_computed_value")
        .or_else(|| node.parameters.get("value"))
        .cloned()
}

fn missing(name: &str) -> RequestError {
    RequestError::NotFound(format!("no document '{}'", name))
}

fn read_only() -> RequestError {
    RequestError::ReadOnly(overseer::access::READ_ONLY.into())
}

#[test]
fn a_bare_name_is_the_askers_own() {
    let (documents, _) = people("bare");
    let kg = |who: &str| {
        documents.as_asker(person(who), || value(&documents.open("weight.os").unwrap(), "kg"))
    };
    assert_eq!(kg("ap31"), Some(OverseerValue::Integer(70)));
    assert_eq!(kg("bob"), Some(OverseerValue::Integer(80)));
    // A subfolder of one's own is one's own, and the full name is the same document.
    documents.as_asker(person("ap31"), || {
        assert!(documents.open("projects/plan.os").is_ok());
        assert!(documents.open("ap31/projects/plan.os").is_ok());
        assert_eq!(documents.named("projects/plan.os"), "ap31/projects/plan.os");
        assert_eq!(documents.named("bob/weight.os"), "bob/weight.os");
    });
}

#[test]
fn another_persons_document_is_as_missing_until_they_share_it() {
    let (documents, root) = people("shared");
    documents.as_asker(person("bob"), || {
        assert_eq!(documents.open("ap31/private.os").err(), Some(missing("ap31/private.os")));
        assert_eq!(documents.open("ap31/nothing.os").err(), Some(missing("ap31/nothing.os")));
        // The server's own files are no one's.
        assert_eq!(documents.open("journal.os").err(), Some(missing("journal.os")));

        assert!(documents.open("ap31/foods.os").is_ok(), "shared to read");
        assert_eq!(
            documents
                .set_at("ap31/foods.os", "Catalog/apple", OverseerValue::String("Pear".into()))
                .err(),
            Some(read_only())
        );
        assert!(documents
            .set_at("ap31/notes.os", "notes/note", OverseerValue::String("from bob".into()))
            .is_ok());
    });
    assert!(std::fs::read_to_string(root.join("ap31/notes.os")).unwrap().contains("from bob"));
    assert!(std::fs::read_to_string(root.join("ap31/foods.os")).unwrap().contains("Apple"));
}

#[test]
fn sharing_os_is_its_owners_alone() {
    let (documents, _) = people("sharing");
    documents.as_asker(person("bob"), || {
        assert_eq!(documents.open("ap31/sharing.os").err(), Some(missing("ap31/sharing.os")));
    });
    documents.as_asker(Asker::Stranger, || {
        assert_eq!(documents.open("ap31/sharing.os").err(), Some(RequestError::SignIn));
    });
    documents.as_asker(person("ap31"), || assert!(documents.open("sharing.os").is_ok()));
}

#[test]
fn what_is_shared_with_everyone_needs_nobody_signed_in() {
    let (documents, root) = people("everyone");
    documents.as_asker(Asker::Stranger, || {
        assert!(documents.open("ap31/recipes.os").is_ok());
        // Not shared and not there are the same answer.
        assert_eq!(documents.open("ap31/private.os").err(), Some(RequestError::SignIn));
        assert_eq!(documents.open("ap31/nothing.os").err(), Some(RequestError::SignIn));

        assert!(documents
            .set_at("ap31/guestbook.os", "guestbook/last", OverseerValue::String("hi".into()))
            .is_ok());
        assert_eq!(
            documents
                .set_at("ap31/recipes.os", "recipes/soup", OverseerValue::String("pea".into()))
                .err(),
            Some(read_only())
        );
        // A press that would change a document shared to be read changes nothing.
        assert_eq!(
            documents.run_event("ap31/recipes.os", "recipes/change", "click").err(),
            Some(read_only())
        );
    });
    assert!(std::fs::read_to_string(root.join("ap31/guestbook.os")).unwrap().contains("hi"));
    assert!(std::fs::read_to_string(root.join("ap31/recipes.os")).unwrap().contains("lentil"));
}

#[test]
fn a_mount_reaches_only_what_its_reader_may_read() {
    let (documents, _) = people("mounts");
    let secret = |who: &str| {
        documents.as_asker(person(who), || {
            let nodes = documents.open("bob/peeks.os").unwrap();
            assert_eq!(value(&nodes, "mine"), Some(OverseerValue::String("bob's".into())));
            value(&nodes, "entry")
        })
    };
    let told = Some(OverseerValue::String("a secret".into()));
    // Either way round: what one was shown is not kept for the other.
    assert_eq!(secret("ap31"), told);
    assert_eq!(secret("bob"), None);
    assert_eq!(secret("ap31"), told);
    assert_eq!(secret("bob"), None);

    // A catalogue shared to be read is mounted whole.
    documents.as_asker(person("bob"), || {
        let nodes = documents.open("weight.os").unwrap();
        assert_eq!(value(&nodes, "apple"), Some(OverseerValue::String("Apple".into())));
    });
}

#[test]
fn a_base_in_another_folder_is_read_on_the_same_terms() {
    let (documents, _) = people("bases");
    let colour = |who: Asker| {
        documents.as_asker(who, || value(&documents.open("bob/card.os").unwrap(), "colour"))
    };
    let red = Some(OverseerValue::String("red".into()));
    assert_eq!(colour(Asker::Anyone), red);
    assert_eq!(colour(person("bob")), None, "ap31's templates are not shared with bob");
    assert_eq!(colour(person("ap31")), red);
}

#[test]
fn a_save_writes_the_document_named_and_nothing_it_mounts() {
    let (documents, root) = people("saves");
    let foods = std::fs::read(root.join("ap31/foods.os")).unwrap();
    documents.as_asker(person("bob"), || {
        let text = std::fs::read_to_string(root.join("bob/weight.os")).unwrap();
        let saved = documents.command(
            Some("weight.os"),
            "save_overseer_file_from_text",
            &serde_json::json!({ "path": "weight.os", "content": text.replace("80", "79") }),
        );
        assert!(saved.is_ok(), "{:?}", saved);
        let refused = documents.command(
            Some("ap31/foods.os"),
            "save_overseer_file_from_text",
            &serde_json::json!({ "path": "ap31/foods.os", "content": "div Catalog { }\n" }),
        );
        assert_eq!(refused.err(), Some(read_only()));
    });
    assert!(std::fs::read_to_string(root.join("bob/weight.os")).unwrap().contains("79"));
    assert_eq!(std::fs::read(root.join("ap31/foods.os")).unwrap(), foods);
}

#[test]
fn the_list_is_mine_shared_and_public() {
    let (documents, _) = people("list");
    let bobs = documents.as_asker(person("bob"), || documents.listing());
    assert_eq!(
        bobs.mine,
        ["bob/card.os", "bob/peeks.os", "bob/sharing.os", "bob/weight.os"]
    );
    assert_eq!(
        bobs.shared.get("ap31").cloned().unwrap_or_default(),
        ["ap31/foods.os", "ap31/guestbook.os", "ap31/notes.os", "ap31/recipes.os"]
    );
    assert_eq!(bobs.public, ["ap31/guestbook.os", "ap31/recipes.os"]);
    assert_eq!(bobs.documents.len(), 8);
    assert!(!bobs.documents.contains(&"journal.os".to_string()));

    let strangers = documents.as_asker(Asker::Stranger, || documents.listing());
    assert_eq!(strangers.documents, ["ap31/guestbook.os", "ap31/recipes.os"]);
    assert!(strangers.mine.is_empty() && strangers.shared.is_empty());
}

#[test]
fn the_service_reaches_the_whole_root() {
    let (documents, _) = people("service");
    let all = documents.list();
    assert!(all.contains(&"journal.os".to_string()));
    assert!(all.contains(&"ap31/sharing.os".to_string()));
    assert!(documents.open("ap31/private.os").is_ok());
    assert!(documents.open("bob/weight.os").is_ok());
}
