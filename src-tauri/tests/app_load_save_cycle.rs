//! The load-and-save cycle, exercised through the code the app actually runs.
//!
//! These call `app_api` directly, which is what the Tauri commands call. Before that split
//! the commands lived in the binary crate where no test could reach them, so every test of
//! this path was a reimplementation of what the commands were assumed to do - and a bug that
//! mangled documents lived in the gap between assumption and behaviour, reproducible by hand
//! and by nothing else.
//!
//! What matters here is the invariant the app promises: loading a document and saving it
//! without touching anything gives back the same bytes, however many times it is repeated.

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::OverseerNode;

/// `SourceRegistry` and the current-document directory are process-wide.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialised<T>(body: impl FnOnce() -> T) -> T {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = body();
    drop(guard);
    out
}

struct Fixture {
    dir: std::path::PathBuf,
}

impl Fixture {
    /// Writes a catalog and a host that mounts it, then points the document manager at the
    /// host - what opening the host file does.
    fn new(host: &str, catalog: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "overseer_cycle_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("foods.os"), catalog).unwrap();
        let host_path = dir.join("tracker.os");
        std::fs::write(&host_path, host).unwrap();
        DocumentManager::set_current_document(Some(host_path.to_string_lossy().as_ref()));
        Fixture { dir }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        DocumentManager::set_current_document(None);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// One load-and-save as the app performs it: load the text, hand the nodes across the IPC
/// boundary (snapshots dropped, ids kept), serialize, then canonicalize as saving does.
fn load_and_save(text: &str) -> String {
    let nodes = app_api::load_document(text.to_string()).expect("load");
    let across_ipc: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&nodes).expect("to json")).expect("from json");
    let serialized = OverseerFileHandler::serialize_nodes(&across_ipc).expect("serialize");
    app_api::canonicalize_document(&serialized)
}

const CATALOG: &str = "// The catalog's own comment.\n\
div food_catalog {\n\
\x20   list Catalog (key=\"handle\") {\n\
\x20       - {\n\
\x20           string handle = \"apple\"\n\
\x20           float kcal = 52\n\
\x20       }\n\
\x20   }\n\
}\n";

const HOST: &str = "// A comment above the document.\n\
tab tracker (label=\"Tracker\") {\n\
\n\
\x20   // A comment inside it.\n\
\x20   mount FOODS (source=\"foods.os/food_catalog/Catalog\", lazy=false, hidden=true) { }\n\
\n\
\x20   div empty { }\n\
\x20   string want = \"apple\"\n\
\x20   float kcal = $(FOODS/Catalog.filter(|x| x/handle == ../want)/kcal)\n\
}\n";

#[test]
fn loading_and_saving_returns_the_same_bytes() {
    serialised(|| {
        let _fx = Fixture::new(HOST, CATALOG);
        assert_eq!(
            load_and_save(HOST),
            HOST,
            "a load-and-save with nothing touched changed the document"
        );
    });
}

/// The damage the app showed was progressive: each save changed a little more. One clean
/// cycle is not enough of a guarantee.
#[test]
fn repeated_load_and_save_is_stable() {
    serialised(|| {
        let _fx = Fixture::new(HOST, CATALOG);
        let mut text = HOST.to_string();
        for pass in 1..=5 {
            text = load_and_save(&text);
            assert_eq!(text, HOST, "the document drifted on save number {}", pass);
        }
    });
}

/// Formatting that only the snapshots can supply: empty blocks, comments, blank lines and
/// the authored order of parameters. Each of these was lost at some point by a save.
#[test]
fn a_save_preserves_authored_formatting() {
    serialised(|| {
        let _fx = Fixture::new(HOST, CATALOG);
        let saved = load_and_save(HOST);

        assert!(
            saved.contains("mount FOODS (source=\"foods.os/food_catalog/Catalog\", lazy=false, hidden=true) { }"),
            "the mount declaration was rewritten:\n{}",
            saved
        );
        assert!(saved.contains("div empty { }"), "an empty block lost its braces:\n{}", saved);
        assert!(saved.contains("// A comment above the document."), "a leading comment was lost");
        assert!(saved.contains("// A comment inside it."), "an interior comment was lost");
        assert!(
            !saved.contains("float kcal = 52"),
            "the mounted catalog leaked into the host:\n{}",
            saved
        );
    });
}

/// The tracker and its catalog, as they actually ship.
#[test]
fn the_real_tracker_survives_repeated_saves() {
    serialised(|| {
        let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/weight_tracker");
        let host_path = examples.join("tracker_v2.os");
        let original = std::fs::read_to_string(&host_path).expect("tracker_v2.os");
        DocumentManager::set_current_document(Some(host_path.to_string_lossy().as_ref()));

        let mut text = original.clone();
        for pass in 1..=3 {
            text = load_and_save(&text);
            assert_eq!(
                text, original,
                "tracker_v2.os drifted on save number {}",
                pass
            );
        }
        DocumentManager::set_current_document(None);
    });
}

/// The app re-evaluates selectively between loading and saving - on a timer, and after any
/// interaction. That path was untestable until the command logic moved into the library, and
/// it is the one a plain load-and-save in the app actually goes through.
fn load_reevaluate_and_save(text: &str) -> String {
    let nodes = app_api::load_document(text.to_string()).expect("load");
    let across_ipc: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&nodes).expect("to json")).expect("from json");

    // What `reevaluateDocumentSelective` does: serialize the live document, hand it back for
    // a selective re-resolve, and adopt the result.
    let serialized = OverseerFileHandler::serialize_nodes(&across_ipc).expect("serialize");
    let resolved = app_api::resolve_selective(serialized, Vec::new(), None).expect("selective");
    let adopted: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&resolved).expect("to json")).expect("from json");

    let saved = OverseerFileHandler::serialize_nodes(&adopted).expect("serialize");
    app_api::canonicalize_document(&saved)
}

#[test]
fn a_selective_reevaluation_between_load_and_save_changes_nothing() {
    serialised(|| {
        let _fx = Fixture::new(HOST, CATALOG);
        assert_eq!(
            load_reevaluate_and_save(HOST),
            HOST,
            "re-evaluating between load and save changed the document"
        );
    });
}

#[test]
fn repeated_reevaluation_and_save_is_stable() {
    serialised(|| {
        let _fx = Fixture::new(HOST, CATALOG);
        let mut text = HOST.to_string();
        for pass in 1..=5 {
            text = load_reevaluate_and_save(&text);
            assert_eq!(text, HOST, "the document drifted on pass {}", pass);
        }
    });
}

#[test]
fn the_real_tracker_survives_reevaluation_and_save() {
    serialised(|| {
        let examples =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/weight_tracker");
        let host_path = examples.join("tracker_v2.os");
        let original = std::fs::read_to_string(&host_path).expect("tracker_v2.os");
        DocumentManager::set_current_document(Some(host_path.to_string_lossy().as_ref()));

        let mut text = original.clone();
        for pass in 1..=3 {
            text = load_reevaluate_and_save(&text);
            assert_eq!(text, original, "tracker_v2.os drifted on pass {}", pass);
        }
        DocumentManager::set_current_document(None);
    });
}
