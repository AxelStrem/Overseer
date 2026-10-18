//! A save keeps the blank line between two top-level tabs.
//!
//! At the top of a document the gap between two nodes is held twice: at the end of the first
//! node's text, and in front of the second. The writer counted only the first, which holds when
//! the first node is given back as it was read - but a press inside a list rebuilds the tab
//! around it, and a rebuilt tab ends at its closing brace. The blank line before the next tab
//! went with it on the first press.

use overseer::server::DocumentRoot;

const DOCUMENT: &str = "div (hidden=true) {
    div Module (layout=\"horizontal\") {
        string name = \"\"
        int inputs = 0
        button add (label=\"Add\") {
            on click {
                inc (path=\"../inputs\") = 1
            }
        }
    }
}

tab catalogue (label=\"Catalogue\", mutable=true) {
    list Modules (entry=<Module>, key=\"name\") {
        - {
            - name = \"Oscillator\"
            - inputs = 2
        }
        - {
            - name = \"Sequencer\"
        }
    }

    button reset (label=\"Reset counts\") {
        on click {
            set (path=\"../Modules/[Oscillator]/inputs\") = 0
        }
    }
}

tab about (label=\"About\") {
    text info = \"A catalogue.\"
}
";

fn a_root(tag: &str, text: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_blanktabs_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the root");
    std::fs::write(root.join("d.os"), text).expect("write");
    root
}

fn press(root: &std::path::Path) -> String {
    let service = DocumentRoot::new(root).expect("open the root");
    service
        .run_event("d.os", "catalogue/Modules/[Sequencer]/add", "click")
        .expect("the press was refused");
    overseer::app_api::forget_baseline();
    std::fs::read_to_string(root.join("d.os")).expect("read")
}

fn keeps_its_lines(tag: &str, newline: &str) {
    let original = DOCUMENT.replace('\n', newline);
    let root = a_root(tag, &original);

    let once = press(&root);
    let pressed = "            - name = \"Sequencer\"\n";
    let expected = original.replace(
        &pressed.replace('\n', newline),
        &format!("{}            - inputs = 1\n", pressed).replace('\n', newline),
    );
    assert_eq!(once, expected, "the first press changed more than the count");

    let twice = press(&root);
    assert_eq!(
        twice,
        expected.replace("- inputs = 1", "- inputs = 2"),
        "the second press changed more than the count"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_press_in_a_list_keeps_the_blank_line_before_the_next_tab() {
    keeps_its_lines("lf", "\n");
}

#[test]
fn a_press_in_a_list_keeps_the_blank_line_before_the_next_tab_with_crlf() {
    keeps_its_lines("crlf", "\r\n");
}
