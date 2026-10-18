//! Colouring a template must not cost it its contents.
//!
//! Styling used to inherit: a `background-color` on a node was copied onto every descendant so
//! the renderer could read it, and each copy was marked `_template_background-color` so it was
//! not written back to disk. That marker said where one *parameter* got its value.
//!
//! The serializer also asks whether a node has any `_template_*` key, and takes a yes to mean
//! the whole node came from a template - which for an entry means "write it without its
//! contents, the template supplies them". Inheriting a colour therefore answered a question it
//! was never asked, and a group inside the coloured template came back as `{}`. A colour stays
//! on the node it is given to now, and these keep it from coming back.

use overseer::app_api;
use overseer::file_ops::OverseerFileHandler;
use overseer::types::OverseerNode;

static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn load_and_save(text: &str) -> String {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    overseer::source_registry::SourceRegistry::reset();
    let nodes = app_api::load_document(text.to_string()).expect("load");
    let across: Vec<OverseerNode> =
        serde_json::from_str(&serde_json::to_string(&nodes).expect("json")).expect("json");
    app_api::canonicalize_document(&OverseerFileHandler::serialize_nodes(&across).expect("save"))
}

/// The shape that failed: a coloured template holding a layout group, the group holding an
/// instance of another template, and a list of entries made from it.
fn document(colour: &str) -> String {
    format!(
        "tab t (label=\"T\", mutable=true) {{
    div (hidden=true) {{
        div Scorer (layout=\"horizontal\", margin=0) {{
            int input (hidden=true) = 0
            int out (label=\"Out\") = $(input * 2)
        }}
        div Card (layout=\"vertical\", margin=0{colour}) {{
            int amount = 1

            div (layout=\"horizontal\", margin=0) {{
                string name (width=30%) = \"x\"

                <Scorer> quality {{
                    - input = $(../../amount)
                }}
            }}
        }}
    }}

    list rows (entry=<Card>) {{
        - {{
            - amount = 3
        }}
    }}
}}
"
    )
}

#[test]
fn the_template_keeps_its_contents_without_a_colour() {
    let plain = document("");
    assert_eq!(load_and_save(&plain), plain);
}

#[test]
fn colouring_a_template_does_not_empty_the_group_inside_it() {
    let coloured = document(", background-color=\"#16203a\"");
    let saved = load_and_save(&coloured);
    assert!(
        !saved.contains("div (layout=\"horizontal\", margin=0) {}"),
        "the group inside the coloured template was written out empty:\n{}",
        saved
    );
    assert!(
        saved.contains("string name (width=30%)"),
        "the group's contents were dropped:\n{}",
        saved
    );
    assert_eq!(saved, coloured, "the coloured document drifted on save");
}

#[test]
fn a_container_colour_is_not_written_onto_children() {
    // The colour belongs to the node that set it, and writing it onto every descendant would
    // make each of them state something the document never said.
    let coloured = document(", background-color=\"#16203a\"");
    let saved = load_and_save(&coloured);
    assert_eq!(
        saved.matches("background-color").count(),
        1,
        "the colour was persisted onto children:\n{}",
        saved
    );
}
