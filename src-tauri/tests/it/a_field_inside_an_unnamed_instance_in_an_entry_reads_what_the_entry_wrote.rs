//! A field inside an unnamed instance in a template reads what an entry, or a named instance,
//! wrote for it.
//!
//! The instance adds no step, so the entry writes `- count = 5` beside its own fields. A resolve
//! pass works from a copy of the document taken at its start, and the template copied for the
//! entry still held `<Box> { }` unexpanded, so `count` was nowhere to be found: the entry's
//! value was kept as a stray field after the inherited ones, and the read found the instance's
//! `count` first, with the template's value.

use crate::common::serial;
use overseer::server::DocumentRoot;
use overseer::types::OverseerValue;
use std::path::PathBuf;

const DOC: &str = r##"tab t (label="T", mutable=true) {
    div (hidden=true) {
        div Box (layout="vertical") {
            int count = 0
        }
        div Row {
            string name = ""
            <Box> { }
            button bump {
                on click {
                    set (path="../count") = 5
                }
            }
        }
    }
    list Rows (entry=<Row>, key="name") {
        - {
            - name = "a"
        }
    }
}
"##;

fn folder(test: &str, text: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_entry_instance_{}_{}", std::process::id(), test));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.os"), text).unwrap();
    root
}

fn read(root: &PathBuf, address: &str) -> Option<OverseerValue> {
    DocumentRoot::new(root).unwrap().read_at("d.os", address).expect("the field").node.parameters.get("value").cloned()
}

#[test]
fn a_press_through_the_instance_is_read_back_from_the_entry() {
    let _guard = serial();
    let root = folder("press", DOC);
    DocumentRoot::new(&root).unwrap().run_event("d.os", "t/Rows/[a]/bump", "click").expect("the press");

    let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(
        saved,
        DOC.replace("- name = \"a\"\n", "- name = \"a\"\n            - count = 5\n"),
        "the value is written into the entry:\n{}",
        saved
    );
    assert_eq!(read(&root, "t/Rows/[a]/count"), Some(OverseerValue::Integer(5)));
}

#[test]
fn a_named_instance_reads_what_it_wrote_for_a_field_inside_the_unnamed_one() {
    let _guard = serial();
    let text = DOC.replace(
        "    list Rows (entry=<Row>, key=\"name\") {\n        - {\n            - name = \"a\"\n        }\n    }\n",
        "    <Row> r {\n        - count = 5\n    }\n",
    );
    let root = folder("named", &text);
    assert_eq!(read(&root, "t/r/count"), Some(OverseerValue::Integer(5)));
}

#[test]
fn a_second_write_changes_the_entrys_line_and_nothing_else() {
    let _guard = serial();
    let written = DOC.replace("- name = \"a\"\n", "- name = \"a\"\n            - count = 5\n");
    let root = folder("again", &written);
    DocumentRoot::new(&root).unwrap().set_at("d.os", "t/Rows/[a]/count", OverseerValue::Integer(7)).expect("the write");

    let saved = std::fs::read_to_string(root.join("d.os")).unwrap();
    assert_eq!(saved, written.replace("count = 5", "count = 7"), "only the entry's line changes:\n{}", saved);
    assert_eq!(read(&root, "t/Rows/[a]/count"), Some(OverseerValue::Integer(7)));
}
