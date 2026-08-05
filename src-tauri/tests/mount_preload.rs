//! Mounting another document as a data source.
//!
//! The calorie tracker needs the food catalog to behave like a lookup table: present as soon
//! as the document opens, resolvable by handle from a formula, invisible in the UI, and never
//! written into the host document's own text.

use overseer::actions::ActionExecutor;
use overseer::docmgr::manager::DocumentManager;
use overseer::file_ops::OverseerFileHandler;
use overseer::formula_evaluator::FormulaEvaluator;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

/// `SourceRegistry` is global and each parse resets it; these tests also set a process-wide
/// document directory, so they must not run concurrently.
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
    /// Writes a catalog and a host document into a scratch directory, then points the
    /// document manager at the host - mirroring what opening the host file does.
    fn new(host_body: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "overseer_mount_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("foods.os"),
            "div food_catalog {\n\
             \x20   string marker = \"ONLY_IN_THE_CATALOG\"\n\
             \x20   list Catalog (key=\"handle\") {\n\
             \x20       - {\n\
             \x20           string handle = \"apple\"\n\
             \x20           float kcal_100g = 52\n\
             \x20       }\n\
             \x20       - {\n\
             \x20           string handle = \"pizza_slice\"\n\
             \x20           float kcal_100g = 340\n\
             \x20       }\n\
             \x20   }\n\
             }\n",
        )
        .unwrap();
        let host = dir.join("tracker.os");
        std::fs::write(&host, host_body).unwrap();
        DocumentManager::set_current_document(Some(host.to_string_lossy().as_ref()));
        Fixture { dir }
    }

    fn host_source(&self) -> String {
        std::fs::read_to_string(self.dir.join("tracker.os")).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        DocumentManager::set_current_document(None);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Mirrors what `parse_overseer_content` does for an opened document.
fn open(source: &str) -> Vec<OverseerNode> {
    overseer::source_registry::SourceRegistry::reset();
    let (_rest, mut nodes) = parser::parse_document(source).expect("host document should parse");
    resolver::resolve_document(&mut nodes);
    ActionExecutor::preload_mounts(&mut nodes);
    resolver::resolve_document(&mut nodes);
    nodes
}

fn find<'a>(nodes: &'a [OverseerNode], path: &[&str]) -> Option<&'a OverseerNode> {
    let mut cur = nodes.iter().find(|n| n.name == path[0])?;
    for seg in &path[1..] {
        cur = cur.children.iter().find(|c| &c.name == seg)?;
    }
    Some(cur)
}

fn value(nodes: &[OverseerNode], path: &[&str]) -> OverseerValue {
    let n = find(nodes, path).unwrap_or_else(|| panic!("missing node {:?}", path.join("/")));
    let owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    FormulaEvaluator::get_effective_value_for_node(n, &owned, nodes)
        .unwrap_or_else(|e| panic!("{:?} failed to evaluate: {:?}", path.join("/"), e))
}

const HOST_PRELOADED: &str = "div tracker {\n\
     \x20   mount FoodDB (source=\"foods.os/food_catalog/Catalog\", lazy=false, hidden=true) { }\n\
     \x20   string want = \"pizza_slice\"\n\
     \x20   float looked_up = $(FoodDB/Catalog.filter(|x| x/handle == ../want).map(|x| x/kcal_100g).first(0.0))\n\
     }\n";

#[test]
fn a_preloaded_mount_is_populated_without_user_action() {
    serialised(|| {
        let fx = Fixture::new(HOST_PRELOADED);
        let nodes = open(&fx.host_source());

        let mount = find(&nodes, &["tracker", "FoodDB"]).expect("mount node");
        assert_eq!(
            mount.parameters.get("_mount_status"),
            Some(&OverseerValue::String("loaded".to_string())),
            "mount should load on open, error was {:?}",
            mount.parameters.get("_mount_error")
        );
        assert!(
            !mount.children.is_empty(),
            "a loaded mount should have materialized children"
        );
    });
}

#[test]
fn a_formula_can_resolve_a_handle_through_a_mount() {
    serialised(|| {
        let fx = Fixture::new(HOST_PRELOADED);
        let nodes = open(&fx.host_source());
        match value(&nodes, &["tracker", "looked_up"]) {
            OverseerValue::Integer(i) => assert_eq!(i, 340, "wrong food resolved"),
            OverseerValue::Float(f) => assert!((f - 340.0).abs() < 0.01, "wrong food resolved"),
            other => panic!("handle lookup across a mount failed: {:?}", other),
        }
    });
}

/// The source is written relative to the document, not to wherever the process runs from.
/// The scratch directory is never the working directory, so this only passes if resolution
/// uses the document's own location.
#[test]
fn a_relative_source_resolves_against_the_document_not_the_process() {
    serialised(|| {
        let fx = Fixture::new(HOST_PRELOADED);
        let cwd = std::env::current_dir().unwrap();
        assert_ne!(
            cwd.canonicalize().ok(),
            fx.dir.canonicalize().ok(),
            "precondition: the fixture directory must not be the working directory"
        );

        let nodes = open(&fx.host_source());
        let mount = find(&nodes, &["tracker", "FoodDB"]).unwrap();
        assert_eq!(
            mount.parameters.get("_mount_status"),
            Some(&OverseerValue::String("loaded".to_string())),
            "relative source failed to resolve: {:?}",
            mount.parameters.get("_mount_error")
        );
    });
}

/// Lazy is still the default: a mount that does not ask to be preloaded stays empty.
#[test]
fn mounts_remain_lazy_by_default() {
    serialised(|| {
        let host = "div tracker {\n\
             \x20   mount FoodDB (source=\"foods.os/food_catalog/Catalog\") { }\n\
             }\n";
        let fx = Fixture::new(host);
        let nodes = open(&fx.host_source());
        let mount = find(&nodes, &["tracker", "FoodDB"]).unwrap();
        assert!(
            mount.children.is_empty(),
            "a mount without lazy=false or preload=true should not load on open"
        );
    });
}

/// Mounted content belongs to the other file and must never leak into the host's text.
#[test]
fn mounted_content_is_not_written_into_the_host_document() {
    serialised(|| {
        let fx = Fixture::new(HOST_PRELOADED);
        let original = fx.host_source();
        let nodes = open(&original);

        let out = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize host");
        // A marker present only in the catalog file. The host's own text mentions
        // "pizza_slice", so that would not distinguish leaked content from source.
        assert!(
            !out.contains("ONLY_IN_THE_CATALOG"),
            "catalog content leaked into the host document:\n{}",
            out
        );
        assert_eq!(out, original, "host document drifted after mounting");
    });
}

/// A missing side file must not stop the host document from opening.
#[test]
fn a_broken_mount_reports_an_error_without_failing_the_document() {
    serialised(|| {
        let host = "div tracker {\n\
             \x20   mount Missing (source=\"nope.os/whatever\", lazy=false) { }\n\
             \x20   string ok = \"still here\"\n\
             }\n";
        let fx = Fixture::new(host);
        let nodes = open(&fx.host_source());

        let mount = find(&nodes, &["tracker", "Missing"]).expect("mount node");
        assert_eq!(
            mount.parameters.get("_mount_status"),
            Some(&OverseerValue::String("error".to_string()))
        );
        assert!(mount.parameters.get("_mount_error").is_some());
        assert_eq!(
            value(&nodes, &["tracker", "ok"]),
            OverseerValue::String("still here".to_string()),
            "the rest of the document should be unaffected"
        );
    });
}
