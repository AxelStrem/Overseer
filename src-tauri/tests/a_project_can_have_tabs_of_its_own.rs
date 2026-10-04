//! A project document can have tabs of its own beside the plan, and they stay out of its way.
//!
//! The sweep that carries finished project work into the task history reads `project/History`,
//! and nothing else. A project can now hold a feature list, notes or a roadmap in tabs of its own -
//! anything at all, even a list that is also called `History` - and none of it may be mistaken
//! for the plan. That rests on one fact about addresses, pinned here: a named tab is a step of the
//! address, so `project/History` is the main tab's list whatever else the document holds.

use overseer::server::DocumentRoot;

const DOCUMENT: &str = r#"tab project (label="Plan", mutable=true) {
    string name = "A project"
    text description (markdown=true) = "The plan, and a tab of its own beside it."

    div (hidden=true) {
        div Finished {
            string title = ""
        }
    }

    list History (entry=<Finished>) {
        - {
            - title = "the plan's own"
        }
    }
}

tab features (label="Features", mutable=true) {
    div (hidden=true) {
        div Finished {
            string title = ""
        }
    }

    list History (entry=<Finished>) {
        - {
            - title = "a decoy"
        }
        - {
            - title = "another decoy"
        }
    }
}

tab (label="Unnamed") {
    list History (entry=<Finished>) {
        - {
            - title = "a decoy with no tab name in its way"
        }
    }
}
"#;

fn documents(tag: &str) -> DocumentRoot {
    let root = std::env::temp_dir().join(format!("overseer_project_tabs_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("projects")).unwrap();
    std::fs::write(root.join("projects").join("p.os"), DOCUMENT).unwrap();
    DocumentRoot::new(&root).unwrap()
}

fn titles(documents: &DocumentRoot, address: &str) -> Vec<String> {
    let view = documents.read_at("projects/p.os", address).expect("read");
    let value = serde_json::to_value(&view).unwrap();
    let text = value.to_string();
    ["the plan's own", "a decoy", "another decoy", "a decoy with no tab name in its way"]
        .iter()
        .filter(|t| text.contains(&format!("\"{}\"", t)))
        .map(|t| t.to_string())
        .collect()
}

#[test]
fn the_plans_history_is_the_main_tabs_alone() {
    let documents = documents("main");
    assert_eq!(titles(&documents, "project/History"), vec!["the plan's own".to_string()]);
}

#[test]
fn a_tab_of_its_own_is_reached_by_its_own_name() {
    let documents = documents("own");
    assert_eq!(titles(&documents, "features/History"), vec!["a decoy".to_string(), "another decoy".to_string()]);
}

#[test]
fn the_main_tab_says_what_the_project_is() {
    let documents = documents("described");
    let view = documents.read_at("projects/p.os", "project/description").expect("read");
    assert!(serde_json::to_value(&view).unwrap().to_string().contains("a tab of its own beside it"));
}
