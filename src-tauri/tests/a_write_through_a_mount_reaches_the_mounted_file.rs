//! A write that lands inside a mount is written to the file the mount was read from.
//!
//! What a mount holds belongs to another file, and the writer gives a mount nothing but `{ }`. So
//! an append into a mounted list changed the copy in memory and was lost, through every door: the
//! bot's, the page's and a press. Two people sharing one shopping list through a mount each is
//! what needs it. `mutable=true` on the mount lets a write through, anything else refuses it, and
//! on a server the person must be able to change the mounted file too.

use overseer::access::Asker;
use overseer::app_api;
use overseer::server::{DocumentRoot, RequestError};
use overseer::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SIDE: &str = r#"tab s (label="S", mutable=true) {
    div (hidden=true) {
        div Thing (layout="horizontal") {
            string handle = ""
            int worth = 0
            int twice = $(worth * 2)
        }
    }

    // Kept as they are.
    list Things (entry=<Thing>, key="handle") {
        - {
            - handle = "a"
            - worth = 1
        }
    }
}
"#;

const WITH_B: &str = r#"tab s (label="S", mutable=true) {
    div (hidden=true) {
        div Thing (layout="horizontal") {
            string handle = ""
            int worth = 0
            int twice = $(worth * 2)
        }
    }

    // Kept as they are.
    list Things (entry=<Thing>, key="handle") {
        - {
            - handle = "a"
            - worth = 1
        }
        - {
            - handle = "b"
            - worth = 5
        }
    }
}
"#;

fn host(mutable: &str) -> String {
    format!(
        concat!(
            "tab t (label=\"T\", mutable=true) {{\n",
            "    mount CATALOG (lazy=false{}, source=\"side.os/s/Things\") {{ }}\n",
            "\n",
            "    button add (label=\"Add\") {{\n",
            "        on click {{\n",
            "            append (list=\"../CATALOG/Things\") {{\n",
            "                - handle = \"c\"\n",
            "                - worth = 3\n",
            "            }}\n",
            "        }}\n",
            "    }}\n",
            "}}\n",
        ),
        mutable
    )
}

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sandbox(tag: &str, mutable: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_mountwrite_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("host.os"), host(mutable)).unwrap();
    std::fs::write(root.join("side.os"), SIDE).unwrap();
    app_api::forget_baseline();
    app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    (Sandbox(root), documents)
}

fn on_disk(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).unwrap()
}

fn a_thing(handle: &str, worth: i64) -> HashMap<String, OverseerValue> {
    [
        ("handle".to_string(), OverseerValue::String(handle.to_string())),
        ("worth".to_string(), OverseerValue::Integer(worth)),
    ]
    .into_iter()
    .collect()
}

#[test]
fn an_append_through_a_mutable_mount_reaches_the_mounted_file() {
    serialised(|| {
        let (sandbox, documents) = sandbox("append", ", mutable=true");
        documents.append_at("host.os", "t/CATALOG/Things", &a_thing("b", 5)).expect("the append");
        assert_eq!(on_disk(&sandbox.0, "side.os"), WITH_B, "the entry is the mounted file's, and nothing else of it moved");
        assert_eq!(on_disk(&sandbox.0, "host.os"), host(", mutable=true"), "the host says what it did");
        let now = documents.open("host.os").expect("reopen");
        assert!(overseer::addressing::find(&now, "t/CATALOG/Things/[b]").is_some(), "the host reads it back");
    });
}

#[test]
fn a_set_through_a_mount_is_refused_unless_it_says_mutable() {
    serialised(|| {
        for (tag, mutable) in [("unsaid", ""), ("fixed", ", mutable=false")] {
            let (sandbox, documents) = sandbox(tag, mutable);
            let refused = documents.set_at("host.os", "t/CATALOG/Things/[a]/worth", OverseerValue::Integer(9)).err();
            assert!(
                matches!(&refused, Some(RequestError::ReadOnly(said)) if said.contains("CATALOG")),
                "{}: {:?}",
                tag,
                refused
            );
            let refused = documents.append_at("host.os", "t/CATALOG/Things", &a_thing("b", 5)).err();
            assert!(matches!(refused, Some(RequestError::ReadOnly(_))), "{}: {:?}", tag, refused);
            assert_eq!(on_disk(&sandbox.0, "side.os"), SIDE, "{}", tag);
            assert_eq!(on_disk(&sandbox.0, "host.os"), host(mutable), "{}", tag);
        }
    });
}

#[test]
fn a_press_that_appends_through_a_mount_is_one_step_in_the_host() {
    serialised(|| {
        let (sandbox, documents) = sandbox("press", ", mutable=true");
        documents.set_at("side.os", "s/Things/[a]/worth", OverseerValue::Integer(2)).expect("a write of the side's own");
        let before = on_disk(&sandbox.0, "side.os");
        documents.run_event("host.os", "t/add", "click").expect("the press");
        assert!(on_disk(&sandbox.0, "side.os").contains("- handle = \"c\""), "the press reached the mounted file");

        let undone = documents.undo("host.os").expect("undo in the host");
        assert_eq!(undone["left_alone"], serde_json::json!([]));
        assert_eq!(on_disk(&sandbox.0, "side.os"), before, "one undo took the press back, and only the press");
        assert_eq!(on_disk(&sandbox.0, "host.os"), host(", mutable=true"));
    });
}

#[test]
fn undo_leaves_a_mounted_file_alone_once_someone_else_has_written_it() {
    serialised(|| {
        let (sandbox, documents) = sandbox("since", ", mutable=true");
        documents.run_event("host.os", "t/add", "click").expect("the press");
        documents.set_at("side.os", "s/Things/[a]/worth", OverseerValue::Integer(4)).expect("someone else's write");
        let theirs = on_disk(&sandbox.0, "side.os");

        let undone = documents.undo("host.os").expect("undo in the host");
        assert_eq!(undone["left_alone"], serde_json::json!(["side.os"]));
        assert_eq!(on_disk(&sandbox.0, "side.os"), theirs, "their write was thrown away");
    });
}

#[test]
fn another_document_mounting_the_same_file_sees_what_was_written() {
    serialised(|| {
        let (sandbox, documents) = sandbox("other", ", mutable=true");
        std::fs::write(sandbox.0.join("other.os"), host("")).unwrap();
        documents.open("other.os").expect("held worked out");
        documents.append_at("host.os", "t/CATALOG/Things", &a_thing("b", 5)).expect("the append");
        let other = documents.open("other.os").expect("reopen");
        assert!(overseer::addressing::find(&other, "t/CATALOG/Things/[b]").is_some());
    });
}

#[test]
fn the_apps_writes_reach_the_mounted_file_too() {
    serialised(|| {
        let (sandbox, _) = sandbox("app", ", mutable=true");
        // The app names a mount's file from the open document; written whole here, so it needs none.
        let side = sandbox.0.join("side.os").to_string_lossy().replace('\\', "/");
        let text = host(", mutable=true").replace("side.os/s/Things", &format!("{}/s/Things", side));
        let file = sandbox.0.join("host.os").to_string_lossy().into_owned();
        std::fs::write(&file, &text).unwrap();

        app_api::append_entry_at(&file, &file, vec!["t".into(), "CATALOG".into(), "Things".into()], a_thing("b", 5), "s")
            .expect("the append");
        assert_eq!(on_disk(&sandbox.0, "side.os"), WITH_B);
        let path = |p: &str| p.split('/').map(String::from).collect::<Vec<_>>();
        app_api::write_values_at(
            &file,
            &file,
            vec![app_api::ValueWrite { node_path: path("t/CATALOG/Things/Thing__2/worth"), value: OverseerValue::Integer(6) }],
            "s",
        )
        .expect("the edit");
        assert!(on_disk(&sandbox.0, "side.os").contains("- worth = 6"));
        assert_eq!(on_disk(&sandbox.0, "host.os"), text);

        let (_, left_alone) = app_api::undo_document(&file).expect("undo");
        assert!(left_alone.is_empty());
        assert_eq!(on_disk(&sandbox.0, "side.os"), WITH_B, "the edit was taken back");
    });
}

fn people(test: &str) -> (DocumentRoot, PathBuf) {
    let root = std::env::temp_dir().join(format!("overseer-mountwrite-access-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let write = |name: &str, text: &str| {
        let file = root.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    };
    write("ap31/sharing.os", concat!(
        "div (hidden=true) {\n",
        "    div Share {\n",
        "        string document = \"\"\n",
        "        string with = \"\"\n",
        "        string can = \"read\"\n",
        "    }\n",
        "}\n",
        "\n",
        "list Shares (entry=<Share>) {\n",
        "    - {\n",
        "        - document = \"foods.os\"\n",
        "        - with = \"bob\"\n",
        "    }\n",
        "}\n",
    ));
    write("ap31/foods.os", concat!(
        "tab foods (mutable=true) {\n",
        "    div (hidden=true) {\n",
        "        div Food {\n",
        "            string handle = \"\"\n",
        "            int kcal = 0\n",
        "        }\n",
        "    }\n",
        "\n",
        "    list Catalog (entry=<Food>, key=\"handle\") {\n",
        "        - {\n",
        "            - handle = \"apple\"\n",
        "            - kcal = 52\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));
    write("bob/weight.os", concat!(
        "tab weight (mutable=true) {\n",
        "    mount FOODS (source=\"../ap31/foods.os/foods/Catalog\", lazy=false, mutable=true, hidden=true) { }\n",
        "\n",
        "    div (hidden=true) {\n",
        "        div Meal {\n",
        "            string food = \"\"\n",
        "            int kcal = $(/weight/FOODS/Catalog.filter(|f| f/handle == ../food).map(|f| f/kcal).sum())\n",
        "        }\n",
        "    }\n",
        "\n",
        "    list Meals (entry=<Meal>) { }\n",
        "}\n",
    ));
    let documents = DocumentRoot::new(&root).unwrap().with_accounts(|name| name == "ap31" || name == "bob");
    (documents, root)
}

#[test]
fn a_write_through_a_mount_needs_edit_on_the_mounted_file() {
    serialised(|| {
        let (documents, root) = people("edit");
        let foods_before = std::fs::read_to_string(root.join("ap31/foods.os")).unwrap();
        documents.as_asker(Asker::Person("bob".into()), || {
            let meal = [("food".to_string(), OverseerValue::String("apple".into()))].into_iter().collect();
            documents.append_at("weight.os", "weight/Meals", &meal).expect("a meal of a food that is there");
            let pear = [
                ("handle".to_string(), OverseerValue::String("pear".into())),
                ("kcal".to_string(), OverseerValue::Integer(57)),
            ]
            .into_iter()
            .collect();
            let refused = documents.append_at("weight.os", "weight/FOODS/Catalog", &pear);
            assert_eq!(refused.err(), Some(RequestError::ReadOnly(overseer::access::READ_ONLY.into())));
        });
        assert!(std::fs::read_to_string(root.join("bob/weight.os")).unwrap().contains("- food = \"apple\""));
        assert_eq!(std::fs::read_to_string(root.join("ap31/foods.os")).unwrap(), foods_before);
        let _ = std::fs::remove_dir_all(&root);
    });
}
