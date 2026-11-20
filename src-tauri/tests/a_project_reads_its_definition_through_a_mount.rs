//! A tracker made from the shipped project template mounts its definition, the document beside
//! it named `<name>.project.os`, read-only under `project/definition`. Whoever may read the
//! definition reads it through the tracker; nobody but its owner writes it, through the mount or
//! by its own name, even with the tracker shared to edit; and whoever may not read it finds the
//! mount missing and the rest of the tracker as it is.

use overseer::access::Asker;
use overseer::app_api;
use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;
use std::path::PathBuf;

const TRACKER: &str = "<projects/project_template.os/project> project (label=\"P\") {\n    - name = \"P\"\n    - definition (source=\"p.project.os/definition\", lazy=false)\n    - Items {\n        - {\n            - added = \"2026-10-01T10:00:00+00:00\"\n            - handle = \"brace\"\n            - title = \"A brace\"\n        }\n    }\n}\n";
const DEFINITION: &str = "<projects/project_definition.os/definition> definition {\n    - description = \"What P is.\"\n    - Repos {\n        - {\n            - name = \"p\"\n            - about = \"Where P is built\"\n        }\n        - {\n            - name = \"site\"\n            - url = \"https://example.com/site.git\"\n            - branch = \"pages\"\n        }\n    }\n    - Tools {\n        - {\n            - file = \"tests/probe.ps1\"\n            - sha256 = \"ab12\"\n            - systems = \"windows\"\n            - setup = true\n        }\n        - {\n            - file = \"tests/check.sh\"\n            - sha256 = \"cd34\"\n        }\n    }\n    - systems = \"linux\"\n}\n";
const SHARED_BOTH: &str = "div (hidden=true) {\n    div Share {\n        string document = \"\"\n        string with = \"\"\n        string can = \"read\"\n    }\n}\n\nlist Shares (entry=<Share>) {\n    - {\n        - document = \"projects/p.os\"\n        - with = \"bob\"\n        - can = \"edit\"\n    }\n    - {\n        - document = \"projects/p.project.os\"\n        - with = \"bob\"\n    }\n}\n";
const DESCRIPTION: &str = "project/definition/definition/description";

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        overseer::document_disk::set_override(None);
    }
}

impl Sandbox {
    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap()
    }
}

fn anns_project(tag: &str, sharing: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_definition_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("ann/projects")).unwrap();
    std::fs::write(root.join("ann/projects/p.os"), TRACKER).unwrap();
    std::fs::write(root.join("ann/projects/p.project.os"), DEFINITION).unwrap();
    std::fs::write(root.join("ann/sharing.os"), sharing).unwrap();
    overseer::document_disk::set_override(Some(None));
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap().with_accounts(|name| name == "ann" || name == "bob");
    (Sandbox(root), documents)
}

fn bob() -> Asker {
    Asker::Person("bob".to_string())
}

fn value_at(documents: &DocumentRoot, asker: Asker, name: &str, address: &str) -> Option<OverseerValue> {
    let view = documents.as_asker(asker, || documents.read_at(name, address)).ok()?;
    view.node.parameters.get("value").cloned()
}

#[test]
fn the_owner_reads_the_definition_through_the_tracker() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = anns_project("owner", SHARED_BOTH);
    let ann = Asker::Person("ann".to_string());
    assert_eq!(
        value_at(&documents, ann.clone(), "ann/projects/p.os", DESCRIPTION),
        Some(OverseerValue::String("What P is.".into()))
    );
    assert_eq!(
        value_at(&documents, ann.clone(), "ann/projects/p.os", "project/definition/definition/Repos/[p]/about"),
        Some(OverseerValue::String("Where P is built".into()))
    );
    assert_eq!(
        value_at(&documents, ann, "ann/projects/p.os", "project/definition/definition/Tools/[tests\\/probe.ps1]/sha256"),
        Some(OverseerValue::String("ab12".into()))
    );
}

#[test]
fn a_repository_says_where_it_is_cloned_from_and_one_that_does_not_reads_empty() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = anns_project("repo_url", SHARED_BOTH);
    let ann = Asker::Person("ann".to_string());
    let repo = |name: &str, field: &str| {
        value_at(&documents, ann.clone(), "ann/projects/p.os", &format!("project/definition/definition/Repos/[{}]/{}", name, field))
    };
    assert_eq!(repo("site", "url"), Some(OverseerValue::String("https://example.com/site.git".into())));
    assert_eq!(repo("site", "branch"), Some(OverseerValue::String("pages".into())));
    assert_eq!(repo("p", "url"), Some(OverseerValue::String(String::new())));
    assert_eq!(repo("p", "branch"), Some(OverseerValue::String(String::new())));
}

#[test]
fn a_tool_marked_setup_reads_so_and_one_that_is_not_reads_false() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = anns_project("tool_setup", SHARED_BOTH);
    let ann = Asker::Person("ann".to_string());
    let setup = |file: &str| {
        value_at(&documents, ann.clone(), "ann/projects/p.os", &format!("project/definition/definition/Tools/[{}]/setup", file))
    };
    assert_eq!(setup("tests\\/probe.ps1"), Some(OverseerValue::Boolean(true)));
    assert_eq!(setup("tests\\/check.sh"), Some(OverseerValue::Boolean(false)));
}

#[test]
fn whoever_may_read_the_definition_reads_it_through_the_tracker() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_sandbox, documents) = anns_project("read", SHARED_BOTH);
    assert_eq!(
        value_at(&documents, bob(), "ann/projects/p.os", DESCRIPTION),
        Some(OverseerValue::String("What P is.".into()))
    );
}

#[test]
fn the_definition_is_not_written_through_the_tracker_even_by_whoever_may_edit_the_tracker() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = anns_project("throughmount", SHARED_BOTH);
    let tracker = sandbox.read("ann/projects/p.os");
    let set = documents.as_asker(bob(), || {
        documents.set_at("ann/projects/p.os", DESCRIPTION, OverseerValue::String("Something else.".into()))
    });
    match set {
        Err(overseer::server::RequestError::ReadOnly(_)) => {}
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
    assert_eq!(sandbox.read("ann/projects/p.project.os"), DEFINITION);
    assert_eq!(sandbox.read("ann/projects/p.os"), tracker);

    // The tracker itself is still theirs to change.
    let renamed = documents.as_asker(bob(), || {
        documents.set_at("ann/projects/p.os", "project/Items/[brace]/title", OverseerValue::String("Renamed".into()))
    });
    assert!(renamed.is_ok(), "{:?}", renamed.err());
}

#[test]
fn the_mount_is_read_only_even_to_the_owner_who_writes_the_definition_by_its_own_name() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = anns_project("ownerwrites", SHARED_BOTH);
    let ann = Asker::Person("ann".to_string());
    let through = documents.as_asker(ann.clone(), || {
        documents.set_at("ann/projects/p.os", DESCRIPTION, OverseerValue::String("Through.".into()))
    });
    assert!(through.is_err(), "a write through the read-only mount was let through");
    assert_eq!(sandbox.read("ann/projects/p.project.os"), DEFINITION);

    let own = documents.as_asker(ann.clone(), || {
        documents.set_at("ann/projects/p.project.os", "definition/description", OverseerValue::String("By name.".into()))
    });
    assert!(own.is_ok(), "{:?}", own.err());
    assert!(sandbox.read("ann/projects/p.project.os").contains("By name."));
    assert_eq!(
        value_at(&documents, ann, "ann/projects/p.os", DESCRIPTION),
        Some(OverseerValue::String("By name.".into()))
    );
}

#[test]
fn the_definition_is_not_written_by_its_own_name_by_whoever_it_is_shared_with_to_read() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = anns_project("byname", SHARED_BOTH);
    let set = documents.as_asker(bob(), || {
        documents.set_at(
            "ann/projects/p.project.os",
            "definition/description",
            OverseerValue::String("Something else.".into()),
        )
    });
    match set {
        Err(overseer::server::RequestError::ReadOnly(_)) => {}
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
    assert_eq!(sandbox.read("ann/projects/p.project.os"), DEFINITION);
}

#[test]
fn whoever_may_not_read_the_definition_finds_the_mount_missing_and_the_tracker_whole() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let only_tracker = SHARED_BOTH.replace(
        "    - {\n        - document = \"projects/p.project.os\"\n        - with = \"bob\"\n    }\n",
        "",
    );
    assert_ne!(only_tracker, SHARED_BOTH);
    let (_sandbox, documents) = anns_project("unshared", &only_tracker);
    assert_eq!(value_at(&documents, bob(), "ann/projects/p.os", DESCRIPTION), None);
    assert_eq!(
        value_at(&documents, bob(), "ann/projects/p.os", "project/Items/[brace]/title"),
        Some(OverseerValue::String("A brace".into()))
    );
}

#[test]
fn a_tracker_without_a_definition_reads_as_it_did() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (sandbox, documents) = anns_project("none", SHARED_BOTH);
    std::fs::remove_file(sandbox.0.join("ann/projects/p.project.os")).unwrap();
    let ann = Asker::Person("ann".to_string());
    assert_eq!(value_at(&documents, ann.clone(), "ann/projects/p.os", DESCRIPTION), None);
    assert_eq!(
        value_at(&documents, ann, "ann/projects/p.os", "project/Items/[brace]/title"),
        Some(OverseerValue::String("A brace".into()))
    );
}

