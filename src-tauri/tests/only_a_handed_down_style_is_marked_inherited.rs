//! A style a node was handed is told apart from one it states.
//!
//! Inheriting a style leaves `_template_<param>` on the node so the writer does not save it,
//! and a list entry's template leaves the same marker on every parameter it states. The page
//! read that marker as "handed down", so a button in an entry that names its own font colour
//! was painted over with the contrast colour of the row it sat on. `_inherited_<param>` is left
//! by inheritance alone.

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
        div Row (layout=\"horizontal\", background-color=\"#1c2a3a\", font-color=\"#ffffff\") {
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

#[test]
fn a_list_entrys_button_keeps_its_own_font_colour_unmarked() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let owns = all_named(&nodes, "own");
    assert!(!owns.is_empty());
    for own in owns {
        assert!(
            !own.parameters.contains_key("_inherited_font-color"),
            "a font colour the button states was marked as handed down"
        );
        assert!(own.parameters.contains_key("_inherited_background-color"));
    }
}

#[test]
fn a_list_entrys_button_is_marked_for_the_font_colour_it_was_handed() {
    let nodes = app_api::load_document(DOCUMENT.to_string()).expect("load");
    let givens = all_named(&nodes, "given");
    assert!(!givens.is_empty());
    for given in givens {
        assert!(given.parameters.contains_key("_inherited_font-color"));
        assert!(given.parameters.contains_key("_inherited_background-color"));
    }
}
