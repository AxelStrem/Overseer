//! A project tracker keeps a bench: `project/Bench`, the loops waiting for work, each entry a
//! `seat` (one loop), the `account` it signs in as and `since` when it sat down. The loops write
//! it through the server, appending a seat and removing it by its key, and the bench beside the
//! filter bar shows each waiting account once, as its chip where the tracker knows the person.
//! A seat holds a `/`, so its address escapes it: `project/Bench/[bot@one\/main]`.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;
use std::collections::HashMap;
use std::path::PathBuf;

const BARE: &str = "<projects/project_template.os/project> project {\n    - name = \"Benched\"\n}\n";

const SEATED: &str = r#"<projects/project_template.os/project> project {
    - name = "Benched"
    - people {
        - People {
            - {
                - tag = "bot"
            }
        }
    }
    - Bench {
        - {
            - seat = "bot@one/main"
            - account = "bot"
            - since = "2026-11-23T10:00:00Z"
        }
        - {
            - seat = "bot@two/main"
            - account = "bot"
            - since = "2026-11-23T09:00:00Z"
        }
        - {
            - seat = "helper@one/main"
            - account = "helper"
            - since = "2026-11-23T11:00:00Z"
        }
    }
}
"#;

fn folder(test: &str, text: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_bench_{}_{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("p.os"), text).unwrap();
    root
}

fn value(service: &DocumentRoot, address: &str) -> Option<OverseerValue> {
    let found = service.read_at("p.os", address).ok()?;
    let params = &found.node.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

fn seats(service: &DocumentRoot) -> usize {
    service.read_at("p.os", "project/Bench").expect("no project/Bench").node.children.len()
}

fn a_seat(seat: &str, account: &str) -> HashMap<String, OverseerValue> {
    let mut fields = HashMap::new();
    fields.insert("seat".to_string(), OverseerValue::String(seat.to_string()));
    fields.insert("account".to_string(), OverseerValue::String(account.to_string()));
    fields.insert("since".to_string(), OverseerValue::String("2026-11-23T12:00:00Z".to_string()));
    fields
}

#[test]
fn a_tracker_that_never_names_the_bench_has_it_empty() {
    let _serial = serial();
    let root = folder("bare", BARE);
    let service = DocumentRoot::new(&root).unwrap();
    assert_eq!(seats(&service), 0);
    assert_eq!(value(&service, "project/bench/waiting"), Some(OverseerValue::String(String::new())));
}

#[test]
fn the_bench_shows_each_waiting_account_once() {
    let _serial = serial();
    let root = folder("seated", SEATED);
    let service = DocumentRoot::new(&root).unwrap();
    assert_eq!(value(&service, "project/bench/waiting"), Some(OverseerValue::String("<@bot> helper".into())));
}

#[test]
fn a_loop_sits_down_and_leaves_by_its_seat() {
    let _serial = serial();
    let root = folder("sit", BARE);
    let service = DocumentRoot::new(&root).unwrap();

    service.append_at("p.os", "project/Bench", &a_seat("bot@one/main", "bot")).expect("the sit was refused");
    service.append_at("p.os", "project/Bench", &a_seat("bot@two/main", "bot")).expect("the second sit was refused");
    assert_eq!(seats(&service), 2);
    assert_eq!(value(&service, "project/bench/waiting"), Some(OverseerValue::String("bot".into())));

    service.remove_at("p.os", r"project/Bench/[bot@one\/main]", &HashMap::new()).expect("the leave was refused");
    let text = std::fs::read_to_string(root.join("p.os")).unwrap();
    assert!(!text.contains("bot@one/main"), "{}", text);
    assert!(text.contains("- seat = \"bot@two/main\""), "{}", text);
    assert_eq!(seats(&service), 1);

    service.remove_at("p.os", r"project/Bench/[bot@two\/main]", &HashMap::new()).expect("the last leave was refused");
    assert_eq!(seats(&service), 0);
    assert_eq!(value(&service, "project/bench/waiting"), Some(OverseerValue::String(String::new())));
}
