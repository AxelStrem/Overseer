//! The page's own pages - the one shown while a document loads, and the one an error is shown on -
//! are Overseer documents, `src/pages/*.os`. They are drawn when there is no server to work them
//! out, the error page above all, so the page carries them worked out already, in the shipped
//! theme, light and dark: `src/pages/*.json`. This works them out again and holds the two to each
//! other. Run with `OVERSEER_WRITE_PAGES=1` to write what it works out, after changing a page or
//! the shipped theme.

use overseer::environment;
use overseer::server::DocumentRoot;
use overseer::types::ForThePage;

const PAGES: &[&str] = &["loading", "error"];

struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pages_folder() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/pages")
}

/// A page worked out in both modes, as the page carries it.
fn worked_out(name: &str) -> String {
    let text = std::fs::read_to_string(pages_folder().join(format!("{}.os", name))).expect("the page's document");
    let root = std::env::temp_dir().join(format!("overseer_pages_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let _sandbox = Sandbox(root.clone());
    let file = format!("{}.os", name);
    std::fs::write(root.join(&file), text).unwrap();
    overseer::app_api::forget_baseline();
    overseer::app_api::forget_dependencies();
    overseer::viewstate::forget_all();
    let documents = DocumentRoot::new(&root).unwrap();
    let mut modes = serde_json::Map::new();
    for mode in ["light", "dark"] {
        let nodes = environment::seen_as(Some(mode), || documents.open(&file)).expect("the page works out");
        modes.insert(mode.to_string(), serde_json::to_value(ForThePage(nodes)).unwrap());
    }
    let mut out = serde_json::to_string_pretty(&serde_json::Value::Object(modes)).unwrap();
    out.push('\n');
    out
}

#[test]
fn each_page_is_carried_as_it_works_out() {
    let writing = std::env::var("OVERSEER_WRITE_PAGES").is_ok_and(|v| v == "1");
    for name in PAGES {
        let now = worked_out(name);
        let carried_at = pages_folder().join(format!("{}.json", name));
        if writing {
            std::fs::write(&carried_at, &now).unwrap();
            continue;
        }
        let carried = std::fs::read_to_string(&carried_at).unwrap_or_default().replace("\r\n", "\n");
        assert!(
            carried == now,
            "src/pages/{}.json is not what src/pages/{}.os works out to: run this test with OVERSEER_WRITE_PAGES=1",
            name,
            name
        );
    }
}
