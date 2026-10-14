//! An image pasted into a document's text is kept beside it with `save_overseer_image`, under
//! `images/<document name>/` and a name picked from the time, and the answer is the path the
//! text refers to it by - one `load_overseer_image` then hands back.

use base64::Engine as _;
use overseer::app_api::{save_image_beside, IconRefused, IMAGE_MOST_BYTES};
use overseer::server::{DocumentRoot, RequestError};
use serde_json::json;
use std::path::PathBuf;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0xFF];

fn encoded(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn a_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_pasted_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("projects")).expect("make the root");
    std::fs::write(root.join("projects/tracker.os"), "tab t {\n    text body = \"\"\n}\n").expect("write");
    root
}

fn save(service: &DocumentRoot, mime: &str, data: &str) -> Result<serde_json::Value, RequestError> {
    service.command_for(
        "alice",
        Some("projects/tracker.os"),
        "save_overseer_image",
        &json!({ "path": "projects/tracker.os", "mime": mime, "data": data }),
    )
}

#[test]
fn a_pasted_image_is_kept_under_the_documents_own_images_folder() {
    let root = a_root("kept");
    let service = DocumentRoot::new(&root).expect("open the root");
    for (mime, extension) in [("image/png", "png"), ("image/jpeg", "jpg"), ("image/webp", "webp"), ("image/gif", "gif")] {
        let path = save(&service, mime, &encoded(PNG)).expect("saved");
        let path = path.as_str().expect("a path");
        assert!(path.starts_with("images/tracker/"), "{}", path);
        assert!(path.ends_with(&format!(".{}", extension)), "{}", path);
        assert_eq!(std::fs::read(root.join("projects").join(path)).expect("on disk"), PNG);
        let back = service
            .command_for(
                "alice",
                Some("projects/tracker.os"),
                "load_overseer_image",
                &json!({ "path": "projects/tracker.os", "image": path }),
            )
            .expect("read back");
        assert_eq!(back, json!({ "mime": mime, "data": encoded(PNG) }));
    }
}

#[test]
fn two_images_pasted_in_the_same_second_keep_both() {
    let root = a_root("twice");
    let document = root.join("projects/tracker.os");
    let first = save_image_beside(&document, "image/png", &encoded(PNG)).expect("first");
    let second = save_image_beside(&document, "image/png", &encoded(&[1, 2, 3])).expect("second");
    let third = save_image_beside(&document, "image/png", &encoded(&[4, 5, 6])).expect("third");
    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_eq!(std::fs::read(root.join("projects").join(&first)).unwrap(), PNG);
    assert_eq!(std::fs::read(root.join("projects").join(&second)).unwrap(), vec![1, 2, 3]);
    assert_eq!(std::fs::read(root.join("projects").join(&third)).unwrap(), vec![4, 5, 6]);
}

#[test]
fn only_an_image_that_could_be_shown_is_kept() {
    let root = a_root("refused");
    let service = DocumentRoot::new(&root).expect("open the root");
    let too_large = encoded(&vec![0u8; IMAGE_MOST_BYTES as usize + 1]);
    for (mime, data) in [
        ("image/svg+xml", encoded(b"<svg/>")),
        ("text/plain", encoded(b"words")),
        ("image/png", "not base64 at all!".to_string()),
        ("image/png", String::new()),
        ("image/png", too_large),
    ] {
        match save(&service, mime, &data) {
            Err(RequestError::Rejected(m)) => assert!(!m.contains("knows how"), "{}", m),
            other => panic!("'{}' was not refused: {:?}", mime, other),
        }
    }
    assert!(!root.join("projects/images/tracker").exists() || std::fs::read_dir(root.join("projects/images/tracker")).unwrap().next().is_none());
    assert!(matches!(
        save_image_beside(&root.join("projects/tracker.os"), "image/bmp", &encoded(PNG)),
        Err(IconRefused::NotAName(_))
    ));
}

#[test]
fn an_image_is_kept_only_beside_a_document_the_server_serves() {
    let root = a_root("outside");
    let service = DocumentRoot::new(&root).expect("open the root");
    for path in ["../tracker.os", "projects/none.os", "projects/notes.txt"] {
        let answer = service.command_for(
            "alice",
            None,
            "save_overseer_image",
            &json!({ "path": path, "mime": "image/png", "data": encoded(PNG) }),
        );
        assert!(answer.is_err(), "'{}' was accepted: {:?}", path, answer);
    }
}
