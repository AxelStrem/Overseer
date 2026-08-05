//! Saving a document that mounts another one.
//!
//! Reproduces the corruption seen after opening `tracker_v2.os` and saving: the mounted
//! catalog's entries appear inside the host, the host's own comments vanish, the catalog's
//! comments arrive in their place, and the indentation is rewritten.
//!
//! The path that matters is the one the app actually takes. A node that has been through
//! the IPC boundary carries only `source_id` - `NodeSourceSnapshot` is `#[serde(skip)]` -
//! so the serializer recovers its text from the `SourceRegistry` by that id. Any test that
//! keeps the in-process snapshots alive silently bypasses the registry and proves nothing
//! about saving.

use overseer::actions::ActionExecutor;
use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::types::OverseerNode;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

const CATALOG: &str = "// CATALOG COMMENT: belongs to foods.os only.\n\
div food_catalog {\n\
\x20   list Catalog (key=\"handle\") {\n\
\x20       - {\n\
\x20           string handle = \"apple\"\n\
\x20           float kcal = 52\n\
\x20       }\n\
\x20   }\n\
}\n\
// CATALOG TRAILING: also belongs to foods.os only.\n";

const HOST: &str = "// HOST COMMENT: belongs to the tracker.\n\
tab tracker (label=\"Tracker\") {\n\
\x20   mount FOODS (source=\"foods.os/food_catalog/Catalog\", lazy=false, hidden=true) { }\n\
\x20   string want = \"apple\"\n\
\x20   float kcal = $(FOODS/Catalog.filter(|x| x/handle == ../want)/kcal)\n\
}\n";

struct Fixture {
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "overseer_iso_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("foods.os"), CATALOG).unwrap();
        let host = dir.join("tracker.os");
        std::fs::write(&host, HOST).unwrap();
        DocumentManager::set_current_document(Some(host.to_string_lossy().as_ref()));
        Fixture { dir }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        DocumentManager::set_current_document(None);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Opens the host the way the app does, then hands the nodes across the IPC boundary
/// exactly as Tauri would: snapshots dropped, ids retained.
fn open_and_cross_ipc() -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(HOST).expect("host parses");
    resolver::resolve_document(&mut nodes);
    ActionExecutor::preload_mounts(&mut nodes);
    resolver::resolve_document(&mut nodes);

    let json = serde_json::to_string(&nodes).expect("to json");
    serde_json::from_str(&json).expect("from json")
}

#[test]
fn saving_a_host_with_a_mount_does_not_pull_in_the_mounted_document() {
    serialised(|| {
        let _fx = Fixture::new();
        let payload = open_and_cross_ipc();
        let out = OverseerFileHandler::serialize_nodes(&payload).expect("serialize host");

        assert!(
            !out.contains("CATALOG COMMENT") && !out.contains("CATALOG TRAILING"),
            "the mounted document's comments were written into the host:\n{}",
            out
        );
        assert!(
            !out.contains("float kcal = 52"),
            "the mounted catalog's entries were written into the host:\n{}",
            out
        );
        assert!(
            out.contains("HOST COMMENT"),
            "the host's own comment was lost:\n{}",
            out
        );
        assert!(
            out.contains("tab tracker"),
            "the host's root node was lost:\n{}",
            out
        );
        assert_eq!(out, HOST, "the host document drifted on save");
    });
}

/// The narrow mechanism: parsing a second document must not invalidate the first one's
/// registered snapshots, because both documents are live at once whenever a mount is used.
#[test]
fn parsing_a_second_document_leaves_the_first_ones_snapshots_intact() {
    serialised(|| {
        overseer::source_registry::SourceRegistry::reset();
        let (_r, host_nodes) = parser::parse_document(HOST).expect("host parses");

        let ids: Vec<String> = host_nodes
            .iter()
            .filter_map(|n| n.source_id.clone())
            .collect();
        assert!(!ids.is_empty(), "host nodes should register snapshots");
        let before: Vec<_> = ids
            .iter()
            .map(|id| overseer::source_registry::SourceRegistry::get(id))
            .collect();

        // Loading a mount parses another document in the same process.
        let (_r2, _catalog) = parser::parse_document(CATALOG).expect("catalog parses");

        for (id, expected) in ids.iter().zip(before.iter()) {
            let now = overseer::source_registry::SourceRegistry::get(id);
            assert_eq!(
                now.as_ref().map(|s| &s.full_text),
                expected.as_ref().map(|s| &s.full_text),
                "snapshot for {} changed after parsing another document - the host would \
                 serialize using the other document's text",
                id
            );
        }
    });
}
