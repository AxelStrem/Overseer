//! A press that cannot do anything must say so.
//!
//! Running an event used to answer the same way whether or not the node had a handler for it:
//! the surrounding node, and no complaint. So pressing `.../bought/click` - the event written
//! onto the end of the address, which resolves, because a handler is a real node - was
//! indistinguishable from pressing `.../bought`. A shopping item was pressed that way three
//! times, each press accepted and journalled, and it never moved to history. The only
//! conclusion available from outside was that the document was broken.
//!
//! Against the real shopping document, because the address that was pressed was a real one.

use overseer::server::{DocumentRoot, RequestError};

struct Sandbox {
    root: std::path::PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sandbox(tag: &str) -> (Sandbox, DocumentRoot) {
    let root = std::env::temp_dir().join(format!("overseer_press_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/shopping/shopping.os");
    std::fs::copy(&source, root.join("shopping.os")).expect("copy the example");
    (Sandbox { root: root.clone() }, DocumentRoot::new(&root).expect("root"))
}

/// The first thing on the list, which is what is pressed to buy it.
fn first_item(documents: &DocumentRoot) -> String {
    let list = documents.read_at("shopping.os", "shopping/List").expect("read the list");
    list.child_addresses.first().expect("the example ships with a list").clone()
}

fn counted(documents: &DocumentRoot, address: &str) -> usize {
    documents.read_at("shopping.os", address).expect("read").child_addresses.len()
}

#[test]
fn the_event_on_the_end_of_the_address_is_refused() {
    let (_dir, documents) = sandbox("handler");
    let item = first_item(&documents);
    let listed = counted(&documents, "shopping/List");
    let bought = counted(&documents, "shopping/History");

    match documents.run_event("shopping.os", &format!("{item}/click"), "click") {
        Err(RequestError::Rejected(said)) => {
            assert!(
                said.contains(&format!("{item}'")),
                "it did not say what to press instead: {said}"
            );
        }
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("a press that did nothing reported success"),
    }

    assert_eq!(counted(&documents, "shopping/List"), listed, "the item should still be listed");
    assert_eq!(counted(&documents, "shopping/History"), bought, "nothing should have been bought");
}

#[test]
fn the_entry_itself_still_works() {
    let (_dir, documents) = sandbox("works");
    let item = first_item(&documents);
    let listed = counted(&documents, "shopping/List");
    let bought = counted(&documents, "shopping/History");

    let outcome = documents
        .run_event("shopping.os", &item, "click")
        .expect("the entry should press");

    assert!(outcome.gone, "the item removed itself, so it should report gone");
    assert_eq!(counted(&documents, "shopping/List"), listed - 1, "it should have left the list");
    assert_eq!(counted(&documents, "shopping/History"), bought + 1, "it should be in history");
}
