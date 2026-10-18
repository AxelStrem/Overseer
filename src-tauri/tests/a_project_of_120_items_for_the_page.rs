//! The project the page's filter is measured against: a hundred and twenty invented items,
//! resolved as the server would hand them over.
//!
//! The page's specs have no resolver of their own, so `tests/filter_at_scale.spec.js` reads the
//! resolved tree from `tests/fixtures/project_120.json.gz`. That file is written here, from the
//! example project with its items replaced, and kept frozen in the repository like any other
//! fixture - compressed, because the resolved tree is several megabytes of repetition. Run this
//! again when the project template changes in a way the filter should see:
//!
//! `cargo test --test a_project_of_120_items_for_the_page -- --ignored`

use overseer::app_api;
use overseer::docmgr::manager::DocumentManager;

const SUBJECTS: &[&str] = &[
    "parser", "brace matching", "row layout", "undo", "filter", "chart", "theme", "drawer",
    "server", "backup", "tab bar", "brace escaping",
];
const VERBS: &[&str] = &[
    "Fix", "Tidy", "Measure", "Document", "Speed up", "Test", "Rethink", "Simplify", "Split",
    "Name",
];
const STAGES: &[&str] = &["filed", "planning", "ready", "testing", "later"];
const LABELS: &[&str] = &["bug", "feature", "interface", "docs"];

fn items() -> String {
    let mut text = String::from("    - Items {\n");
    for n in 0..120 {
        let subject = SUBJECTS[n % SUBJECTS.len()];
        let verb = VERBS[(n / SUBJECTS.len()) % VERBS.len()];
        text.push_str(&format!(
            "        - {{\n            - added = \"2026-09-{:02}T09:{:02}:00+04:00\"\n            - handle = \"item{n}\"\n            - title = \"{verb} the {subject}, part {}\"\n            - stage = \"{}\"\n            - labels = \"{}\"\n            - points = {}\n        }}\n",
            1 + n % 28,
            n % 60,
            n / SUBJECTS.len() + 1,
            STAGES[n % STAGES.len()],
            LABELS[n % LABELS.len()],
            1 + n % 5,
        ));
    }
    text.push_str("    }\n");
    text
}

#[test]
#[ignore = "writes tests/fixtures/project_120.json.gz for the page's specs"]
fn write_the_project_of_120_items() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let folder = here.join("../examples/projects");
    let example = std::fs::read_to_string(folder.join("project.os")).expect("the example project");
    let example = example.replace("\r\n", "\n");

    // The example's own items give way to the invented ones; its labels and the rest stay.
    let start = example.find("    - Items {\n").expect("the example's items");
    let end = start + example[start..].find("\n    }\n").expect("the end of its items") + 7;
    let text = format!("{}{}{}", &example[..start], items(), &example[end..]);

    let nodes = DocumentManager::with_document(Some(folder), || app_api::load_document(text))
        .expect("the project resolves");
    let json = serde_json::to_string(&nodes).unwrap();
    let out = here.join("../tests/fixtures/project_120.json.gz");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let file = std::fs::File::create(&out).unwrap();
    let mut gz = flate2::write::GzEncoder::new(file, flate2::Compression::best());
    std::io::Write::write_all(&mut gz, json.as_bytes()).unwrap();
    gz.finish().unwrap();
}
