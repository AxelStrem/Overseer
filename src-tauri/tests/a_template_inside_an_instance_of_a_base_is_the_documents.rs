//! An instance of another file's node brings that node, and all it holds, into the document: a
//! template inside it is named as one of the document's own - `<Synth>` inside the instance, or
//! after it - so a file names the base's version once, at the top. What the base keeps outside
//! that node is not brought in.

use overseer::server::DocumentRoot;
use overseer::types::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const BASE: &str = r##"div (hidden=true) {
    div Outer {
        int vol = 9
    }
    div patch {
        div (hidden=true) {
            div Synth {
                int id = 0
                float vol = 1.0
            }
        }
        list Processors {
        }
    }
}
"##;

const DERIVED: &str = r##"<brocessor/v200.os/patch> patch {
    - Processors {
        <Synth> s1 {
            - id = 12
        }
    }
}
tab t (mutable=true) {
    <patch/Synth> s2 {
        - id = 3
    }
    list Synths (entry=<Synth>, key="id") {
        - {
            - id = 7
        }
    }
    <Outer> o {
    }
}
"##;

fn folder(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "overseer_brought_in_{}_{}",
        std::process::id(),
        test
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("brocessor")).unwrap();
    std::fs::write(root.join("brocessor/v200.os"), BASE).unwrap();
    std::fs::write(root.join("d.os"), DERIVED).unwrap();
    root
}

fn read(root: &Path, address: &str) -> Option<OverseerValue> {
    let service = DocumentRoot::new(root).unwrap();
    let found = service.read_at("d.os", address).ok()?;
    let params = &found.node.parameters;
    params.get("_computed_value").or_else(|| params.get("value")).cloned()
}

#[test]
fn a_bare_reference_inside_the_instance_names_the_bases_template() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("inside");
    assert_eq!(read(&root, "patch/Processors/s1/vol"), Some(OverseerValue::Float(1.0)));
    assert_eq!(read(&root, "patch/Processors/s1/id"), Some(OverseerValue::Integer(12)));
}

#[test]
fn a_reference_after_the_instance_names_it_too() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("after");
    assert_eq!(read(&root, "t/s2/vol"), Some(OverseerValue::Float(1.0)));
    assert_eq!(read(&root, "t/s2/id"), Some(OverseerValue::Integer(3)));
    assert_eq!(read(&root, "t/Synths/[7]/vol"), Some(OverseerValue::Float(1.0)));
}

#[test]
fn what_the_base_keeps_outside_the_instanced_node_is_not_brought_in() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("outside");
    assert_eq!(read(&root, "t/o/vol"), None);
}

#[test]
fn a_save_keeps_the_bare_reference_as_written() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = folder("save");
    let service = DocumentRoot::new(&root).unwrap();
    service
        .set_at("d.os", "patch/Processors/s1/vol", OverseerValue::Float(0.5))
        .expect("the set was refused");
    service
        .set_at("d.os", "t/Synths/[7]/vol", OverseerValue::Float(0.25))
        .expect("the set was refused");
    let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(
        saved,
        DERIVED
            .replace(
                "            - id = 12\n",
                "            - id = 12\n            - vol = 0.5\n",
            )
            .replace("            - id = 7\n", "            - id = 7\n            - vol = 0.25\n")
    );
}
