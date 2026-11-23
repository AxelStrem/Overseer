//! A document can name a sound beside it, `audio memo = "sounds/memo.webm"` or
//! `![memo](sounds/memo.webm)`, and the page asks for it with `load_overseer_audio`; a sound
//! dropped into a text or recorded in its editor is kept with `save_overseer_audio` under
//! `sounds/<document name>/`. Both are checked as an image is.

use crate::common::{serial, shared};
use base64::Engine as _;
use overseer::app_api::{audio_beside, save_audio_beside, IconRefused, AUDIO_MOST_BYTES};
use overseer::server::{DocumentRoot, RequestError};
use serde_json::json;
use std::path::PathBuf;

const OGG: &[u8] = b"OggS\x00\x02\x00\x00\x00\x00";

fn encoded(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn a_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_sounds_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("notes/sounds")).expect("make the root");
    std::fs::write(root.join("notes/journal.os"), "tab t {\n    audio memo = \"sounds/memo.ogg\"\n}\n").expect("write");
    std::fs::write(root.join("notes/sounds/memo.ogg"), OGG).expect("write");
    std::fs::write(root.join("notes/sounds/Song.MP3"), OGG).expect("write");
    std::fs::write(root.join("notes/sounds/take.webm"), OGG).expect("write");
    std::fs::write(root.join("notes/sounds/plate.png"), OGG).expect("write");
    std::fs::write(root.join("notes/sounds/huge.wav"), vec![0u8; AUDIO_MOST_BYTES as usize + 1]).expect("write");
    std::fs::write(root.join("upstairs.ogg"), OGG).expect("write");
    root
}

fn load(service: &DocumentRoot, audio: &str) -> Result<serde_json::Value, RequestError> {
    service.command_for(
        "alice",
        Some("notes/journal.os"),
        "load_overseer_audio",
        &json!({ "path": "notes/journal.os", "audio": audio }),
    )
}

fn save(service: &DocumentRoot, mime: &str, data: &str) -> Result<serde_json::Value, RequestError> {
    service.command_for(
        "alice",
        Some("notes/journal.os"),
        "save_overseer_audio",
        &json!({ "path": "notes/journal.os", "mime": mime, "data": data }),
    )
}

#[test]
fn the_server_hands_out_a_sound_beside_the_document_with_its_type() {
    let _shared = shared();
    let root = a_root("served");
    let service = DocumentRoot::new(&root).expect("open the root");
    assert_eq!(load(&service, "sounds/memo.ogg"), Ok(json!({ "mime": "audio/ogg", "data": encoded(OGG) })));
    assert_eq!(load(&service, "sounds/Song.MP3").unwrap()["mime"], json!("audio/mpeg"));
    assert_eq!(load(&service, "sounds/take.webm").unwrap()["mime"], json!("audio/webm"));
}

#[test]
fn the_server_plays_no_sound_outside_the_documents_folder_or_of_another_kind() {
    let _shared = shared();
    let root = a_root("refused");
    let service = DocumentRoot::new(&root).expect("open the root");
    let outside = root.join("upstairs.ogg").to_string_lossy().replace('\\', "/");
    for audio in ["../upstairs.ogg", outside.as_str(), "sounds/plate.png", "sounds/huge.wav", "journal.os"] {
        assert!(matches!(load(&service, audio), Err(RequestError::Rejected(_))), "'{}' was not refused", audio);
    }
    assert!(matches!(load(&service, "sounds/none.ogg"), Err(RequestError::NotFound(_))));
    assert!(matches!(audio_beside(&root.join("notes/journal.os"), "sounds/none.ogg"), Err(IconRefused::Missing(m)) if m == "no sound 'sounds/none.ogg'"));
}

#[test]
fn a_recorded_sound_is_kept_under_the_documents_own_sounds_folder() {
    let _shared = shared();
    let root = a_root("kept");
    let service = DocumentRoot::new(&root).expect("open the root");
    for (mime, extension, served) in [
        ("audio/webm;codecs=opus", "webm", "audio/webm"),
        ("audio/ogg; codecs=opus", "ogg", "audio/ogg"),
        ("audio/mp4", "m4a", "audio/mp4"),
        ("audio/mpeg", "mp3", "audio/mpeg"),
        ("audio/wav", "wav", "audio/wav"),
    ] {
        let path = save(&service, mime, &encoded(OGG)).expect("saved");
        let path = path.as_str().expect("a path");
        assert!(path.starts_with("sounds/journal/"), "{}", path);
        assert!(path.ends_with(&format!(".{}", extension)), "{}", path);
        assert_eq!(load(&service, path), Ok(json!({ "mime": served, "data": encoded(OGG) })));
    }
    let first = save_audio_beside(&root.join("notes/journal.os"), "audio/ogg", &encoded(&[1])).expect("first");
    let second = save_audio_beside(&root.join("notes/journal.os"), "audio/ogg", &encoded(&[2])).expect("second");
    assert_ne!(first, second);
}

#[test]
fn only_a_sound_that_could_be_played_is_kept() {
    let _shared = shared();
    let root = a_root("not_kept");
    let service = DocumentRoot::new(&root).expect("open the root");
    let too_large = encoded(&vec![0u8; AUDIO_MOST_BYTES as usize + 1]);
    for (mime, data) in [
        ("image/png", encoded(OGG)),
        ("audio/midi", encoded(OGG)),
        ("audio/ogg", "not base64 at all!".to_string()),
        ("audio/ogg", String::new()),
        ("audio/ogg", too_large),
    ] {
        assert!(matches!(save(&service, mime, &data), Err(RequestError::Rejected(_))), "'{}' was not refused", mime);
    }
    assert!(!root.join("notes/sounds/journal").exists() || std::fs::read_dir(root.join("notes/sounds/journal")).unwrap().next().is_none());
    let answer = service.command_for(
        "alice",
        None,
        "save_overseer_audio",
        &json!({ "path": "../journal.os", "mime": "audio/ogg", "data": encoded(OGG) }),
    );
    assert!(answer.is_err());
}

#[test]
fn an_audio_node_is_read_with_its_path_and_written_back_as_it_was() {
    let _serial = serial();
    use overseer::file_ops::OverseerFileHandler;
    use overseer::{parser, resolver};
    overseer::source_registry::SourceRegistry::reset();
    let original = "tab notes {\n    audio memo (label=\"Voice memo\") = \"sounds/memo.ogg\"\n    text note = \"![memo](sounds/memo.ogg)\"\n}\n";
    let (_rest, mut nodes) = parser::parse_document(original).expect("parses");
    resolver::resolve_document(&mut nodes);
    let memo = &nodes[0].children[0];
    assert_eq!(memo.node_type, "audio");
    assert_eq!(memo.name, "memo");
    assert_eq!(OverseerFileHandler::serialize_nodes(&nodes).expect("serializes"), original);
}
