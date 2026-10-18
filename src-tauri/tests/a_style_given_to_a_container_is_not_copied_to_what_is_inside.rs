//! A colour or font given to a node is that node's alone.
//!
//! A container that says `background-color`, `font-color` or `font-size` paints itself with it,
//! and what is inside it keeps the theme. Copied down, as it used to be, every descendant held a
//! style it never stated, and needed markers saying so: one for the writer, so the copy was not
//! saved, and another for the page, so that a button that names its own font colour was not taken
//! for one handed its row's. A container that wants what is inside it to follow says so with
//! `context theme`.

use overseer::app_api;
use overseer::types::OverseerNode;

fn find<'a>(nodes: &'a [OverseerNode], name: &str, out: &mut Vec<&'a OverseerNode>) {
    for n in nodes {
        if n.name == name {
            out.push(n);
        }
        find(&n.children, name, out);
    }
}

fn all_named<'a>(nodes: &'a [OverseerNode], name: &str) -> Vec<&'a OverseerNode> {
    let mut out = Vec::new();
    find(nodes, name, &mut out);
    out
}

const DOCUMENT: &str = "tab t (label=\"T\", mutable=true) {
    div (hidden=true) {
        div Row (layout=\"horizontal\", background-color=\"#1c2a3a\", font-color=\"#ffffff\", font-size=20px) {
            string name = \"\"
            button own (label=\"own\", font-color=\"#6b7280\") {
                on click {
                    set (path=\"../name\", value=\"x\")
                }
            }
            button given (label=\"given\") {
                on click {
                    set (path=\"../name\", value=\"y\")
                }
            }
        }
    }

    list rows (entry=<Row>) {
        - {
            - name = \"a\"
        }
    }
}
";

const STYLES: [&str; 3] = ["background-color", "font-color", "font-size"];

#[test]
fn an_entry_keeps_the_style_its_template_gives_it() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let rows = all_named(&nodes, "rows");
    let entry = &rows[0].children[0];
    for style in STYLES {
        assert!(entry.parameters.contains_key(style), "the entry lost its {}", style);
    }
}

#[test]
fn nothing_inside_a_styled_container_is_handed_its_style() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    for name in ["name", "given"] {
        let found = all_named(&nodes, name);
        assert!(!found.is_empty());
        for node in found {
            for style in STYLES {
                assert!(!node.parameters.contains_key(style), "{} was handed {}", name, style);
            }
        }
    }
}

#[test]
fn a_button_keeps_the_font_colour_it_states() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let owns = all_named(&nodes, "own");
    assert!(!owns.is_empty());
    for own in owns {
        assert!(own.parameters.contains_key("font-color"));
        assert!(!own.parameters.contains_key("background-color"));
    }
}

#[test]
fn a_styled_container_saves_as_it_was_written() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let saved = overseer::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("save");
    assert_eq!(app_api::canonicalize_document(&saved), DOCUMENT);
}
