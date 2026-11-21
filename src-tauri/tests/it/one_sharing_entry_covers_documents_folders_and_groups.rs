//! One entry of a `sharing.os` covers several documents, a folder, several accounts or a group of
//! them, so handing a project to someone is one entry rather than one per document per account.
//! Every entry is asked, and the widest `can` any of them grants wins.

use crate::common::shared;
use overseer::access::{allows, Asker, Need};
use overseer::server::DocumentRoot;
use std::path::{Path, PathBuf};

const SHARING: &str = r#"div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
    div Group {
        string name = ""
        string members = ""
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "projects/"
        - with = "team"
        - can = "edit"
    }
    - {
        - document = "plan.os, notes.os"
        - with = "carol, dave"
    }
    - {
        - document = "plan.os"
        - with = "carol"
        - can = "edit"
    }
    - {
        - document = "../bob/weight.os, /, ., projects/../private.os"
        - with = "everyone"
        - can = "edit"
    }
    - {
        - document = "/recipes/"
        - with = "everyone"
    }
}

list Groups (entry=<Group>, key="name") {
    - {
        - name = "team"
        - members = "bob, Erin, everyone"
    }
}
"#;

fn root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("overseer-share-sets-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (file, text) in [
        ("ap31/sharing.os", SHARING),
        ("ap31/projects/game.os", "int a = 1\n"),
        ("ap31/projects/deep/inner.os", "int a = 1\n"),
        ("ap31/projects/sharing.os", "int a = 1\n"),
        ("ap31/projects/game.parts/Items/one.os", "int a = 1\n"),
        ("ap31/plan.os", "int a = 1\n"),
        ("ap31/plan.parts/Items/one.os", "int a = 1\n"),
        ("ap31/notes.os", "int a = 1\n"),
        ("ap31/private.os", "int a = 1\n"),
        ("ap31/recipes/soup.os", "int a = 1\n"),
        ("bob/weight.os", "int a = 1\n"),
    ] {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir.canonicalize().unwrap()
}

fn person(name: &str) -> Asker {
    Asker::Person(name.to_string())
}

fn may(root: &Path, asker: &Asker, file: &str, need: Need) -> bool {
    allows(root, asker, Path::new(file), need)
}

#[test]
fn a_folder_share_covers_every_document_under_it_now_and_later() {
    let _shared = shared();
    let root = root("folder");
    let bob = person("bob");
    assert!(may(&root, &bob, "ap31/projects/game.os", Need::Edit));
    assert!(may(&root, &bob, "ap31/projects/deep/inner.os", Need::Edit));
    assert!(may(&root, &bob, "ap31/projects/made-later.os", Need::Edit), "a document made after the share is not covered");
    assert!(!may(&root, &bob, "ap31/plan.os", Need::Read), "a folder share reached outside its folder");
    assert!(!may(&root, &bob, "ap31/projects.os", Need::Read), "a folder share reached a document named like it");
}

#[test]
fn a_folder_share_reaches_the_parts_of_its_documents_only_to_read() {
    let _shared = shared();
    let root = root("parts");
    let bob = person("bob");
    assert!(may(&root, &bob, "ap31/projects/game.parts/Items/one.os", Need::Read));
    assert!(
        !may(&root, &bob, "ap31/projects/game.parts/Items/one.os", Need::Edit),
        "a part was opened to writing by its own name"
    );
}

#[test]
fn a_set_of_documents_is_shared_by_one_entry_with_several_accounts() {
    let _shared = shared();
    let root = root("set");
    for who in ["carol", "dave"] {
        assert!(may(&root, &person(who), "ap31/notes.os", Need::Read), "{} cannot read notes", who);
        assert!(may(&root, &person(who), "ap31/plan.os", Need::Read), "{} cannot read the plan", who);
        assert!(may(&root, &person(who), "ap31/plan.parts/Items/one.os", Need::Read));
    }
    assert!(!may(&root, &person("dave"), "ap31/notes.os", Need::Edit));
    assert!(!may(&root, &person("carol"), "ap31/private.os", Need::Read));
}

#[test]
fn where_entries_overlap_the_widest_can_wins() {
    let _shared = shared();
    let root = root("overlap");
    assert!(may(&root, &person("carol"), "ap31/plan.os", Need::Edit), "an edit share lost to a read share");
    assert!(!may(&root, &person("dave"), "ap31/plan.os", Need::Edit));
}

#[test]
fn a_group_shares_with_its_members_and_never_with_an_account_of_its_name() {
    let _shared = shared();
    let root = root("group");
    assert!(may(&root, &person("erin"), "ap31/projects/game.os", Need::Edit));
    assert!(!may(&root, &person("team"), "ap31/projects/game.os", Need::Read), "an account named as a group was let in");
    assert!(!may(&root, &Asker::Stranger, "ap31/projects/game.os", Need::Read), "a group made everyone a member");
    assert!(!may(&root, &person("zoe"), "ap31/projects/game.os", Need::Read));
}

#[test]
fn a_name_that_climbs_out_or_names_the_whole_folder_covers_nothing() {
    let _shared = shared();
    let root = root("climb");
    assert!(!may(&root, &Asker::Stranger, "bob/weight.os", Need::Read));
    assert!(!may(&root, &Asker::Stranger, "ap31/private.os", Need::Read));
    assert!(!may(&root, &Asker::Stranger, "ap31/plan.os", Need::Read));
    assert!(may(&root, &Asker::Stranger, "ap31/recipes/soup.os", Need::Read), "a folder written from the top was not read");
}

#[test]
fn the_sharing_document_stays_its_owners_under_any_share() {
    let _shared = shared();
    let root = root("sharing");
    assert!(!may(&root, &person("bob"), "ap31/sharing.os", Need::Read));
    // Only the one at the top of the folder is the sharing document; another of that name is not.
    assert!(may(&root, &person("bob"), "ap31/projects/sharing.os", Need::Read));
}

#[test]
fn the_listing_and_the_sharers_reach_show_what_each_entry_comes_to() {
    let _shared = shared();
    let root = root("reach");
    let documents = DocumentRoot::new(&root).unwrap().with_accounts(|name| ["ap31", "bob", "carol"].contains(&name));
    let listing = documents.as_asker(person("bob"), || documents.listing());
    assert_eq!(
        listing.shared.get("ap31").cloned().unwrap_or_default(),
        vec!["ap31/projects/deep/inner.os", "ap31/projects/game.os", "ap31/projects/sharing.os", "ap31/recipes/soup.os"]
    );
    assert_eq!(listing.public, vec!["ap31/recipes/soup.os"]);

    let reach = documents.sharing_reach("ap31");
    assert_eq!(reach.len(), 5);
    assert_eq!(reach[0].people, vec!["bob", "erin"]);
    assert_eq!(reach[0].documents, vec!["ap31/projects/deep/inner.os", "ap31/projects/game.os", "ap31/projects/sharing.os"]);
    assert_eq!(reach[1].document, "plan.os, notes.os");
    assert_eq!(reach[1].documents, vec!["ap31/notes.os", "ap31/plan.os"]);
    assert_eq!(reach[1].people, vec!["carol", "dave"]);
    assert!(reach[3].documents.is_empty(), "{:?}", reach[3].documents);
    assert_eq!(reach[4].documents, vec!["ap31/recipes/soup.os"]);
    assert_eq!(reach[4].people, vec!["everyone"]);
}

#[test]
fn a_one_document_one_account_entry_means_what_it_meant() {
    let _shared = shared();
    let shares = overseer::access::read_shares(
        "div (hidden=true) {\n    div Share {\n        string document = \"\"\n        string with = \"\"\n        string can = \"read\"\n    }\n}\n\nlist Shares (entry=<Share>) {\n    - {\n        - document = \"/shopping.os\"\n        - with = \"Alice\"\n    }\n}\n",
    );
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].document, "shopping.os");
    assert_eq!(shares[0].with, "alice");
    assert_eq!(shares[0].can, "read");
    assert!(overseer::access::covers("shopping.os", "shopping.os"));
    assert!(!overseer::access::covers("shopping.os", "shopping.os/x.os"));
}
