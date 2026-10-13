//! A document can name a raster image beside it, `image plate = "images/plate.png"` or
//! `![plate](images/plate.png)`, and the page asks for it with `load_overseer_image`. Both doors
//! answer from the document's own folder and below, with the image's type and its bytes in
//! base64, and from nowhere else.

use overseer::app_api::{image_beside, IconRefused, IMAGE_MOST_BYTES};
use overseer::server::{DocumentRoot, RequestError};
use serde_json::json;
use std::path::PathBuf;

/// The eight bytes every PNG starts with, and a few after: enough to tell the bytes came back
/// as they were.
const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0xFF];
const PNG_BASE64: &str = "iVBORw0KGgoAAAAN/w==";

/// A root with `kitchen/pantry.os`, its images beside it, and an image outside the kitchen it may
/// not reach.
fn a_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_images_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("kitchen/images")).expect("make the root");
    std::fs::write(root.join("kitchen/pantry.os"), "tab pantry {\n    image plate = \"images/plate.png\"\n}\n").expect("write");
    std::fs::write(root.join("kitchen/images/plate.png"), PNG).expect("write");
    std::fs::write(root.join("kitchen/images/Bowl.JPG"), PNG).expect("write");
    std::fs::write(root.join("kitchen/images/jar.webp"), PNG).expect("write");
    std::fs::write(root.join("kitchen/images/spin.gif"), PNG).expect("write");
    std::fs::write(root.join("kitchen/images/icon.svg"), "<svg/>").expect("write");
    std::fs::write(root.join("kitchen/images/notes.txt"), "not an image").expect("write");
    std::fs::write(root.join("kitchen/images/huge.png"), vec![0u8; IMAGE_MOST_BYTES as usize + 1]).expect("write");
    std::fs::write(root.join("upstairs.png"), PNG).expect("write");
    root
}

fn ask(service: &DocumentRoot, image: &str) -> Result<serde_json::Value, RequestError> {
    service.command_for(
        "alice",
        Some("kitchen/pantry.os"),
        "load_overseer_image",
        &json!({ "path": "kitchen/pantry.os", "image": image }),
    )
}

#[test]
fn the_server_hands_out_an_image_beside_the_document_with_its_type() {
    let root = a_root("served");
    let service = DocumentRoot::new(&root).expect("open the root");
    assert_eq!(ask(&service, "images/plate.png"), Ok(json!({ "mime": "image/png", "data": PNG_BASE64 })));
    assert_eq!(ask(&service, "images/Bowl.JPG"), Ok(json!({ "mime": "image/jpeg", "data": PNG_BASE64 })));
    assert_eq!(ask(&service, "images/jar.webp").unwrap()["mime"], json!("image/webp"));
    assert_eq!(ask(&service, "images/spin.gif").unwrap()["mime"], json!("image/gif"));
}

#[test]
fn the_server_reads_no_image_outside_the_documents_folder() {
    let root = a_root("refused");
    let service = DocumentRoot::new(&root).expect("open the root");
    let outside = root.join("upstairs.png").to_string_lossy().replace('\\', "/");
    for image in [
        "../upstairs.png",
        "images/../../upstairs.png",
        "/upstairs.png",
        "C:/upstairs.png",
        outside.as_str(),
        "./images/plate.png",
        "",
        "images/icon.svg",
        "images/notes.txt",
        "images/huge.png",
        "pantry.os",
    ] {
        assert!(
            matches!(ask(&service, image), Err(RequestError::Rejected(_))),
            "'{}' was not refused: {:?}",
            image,
            ask(&service, image)
        );
    }
    assert!(matches!(ask(&service, "images/none.png"), Err(RequestError::NotFound(_))));
}

#[test]
fn the_desktop_app_reads_images_beside_the_open_file_as_the_server_does() {
    let root = a_root("desktop");
    let document = root.join("kitchen/pantry.os");
    assert_eq!(image_beside(&document, "images/plate.png"), Ok(json!({ "mime": "image/png", "data": PNG_BASE64 })));
    for image in ["../upstairs.png", "/upstairs.png", "images/notes.txt", "images/huge.png"] {
        assert!(matches!(image_beside(&document, image), Err(IconRefused::NotAName(_))), "'{}' was not refused", image);
    }
    assert!(image_beside(&document, root.join("upstairs.png").to_str().unwrap()).is_err());
    assert!(matches!(image_beside(&document, "images/none.png"), Err(IconRefused::Missing(_))));
}

#[test]
fn an_image_node_is_read_with_its_path_and_written_back_as_it_was() {
    use overseer::file_ops::OverseerFileHandler;
    use overseer::{parser, resolver};
    overseer::source_registry::SourceRegistry::reset();
    let original = "tab pantry {\n    image plate (width=50%) = \"images/plate.png\"\n    text note = \"![plate](images/plate.png)\"\n}\n";
    let (_rest, mut nodes) = parser::parse_document(original).expect("parses");
    resolver::resolve_document(&mut nodes);
    let plate = &nodes[0].children[0];
    assert_eq!(plate.node_type, "image");
    assert_eq!(plate.name, "plate");
    assert!(plate.parameters.contains_key("width"));
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).expect("serializes"), original);
}
