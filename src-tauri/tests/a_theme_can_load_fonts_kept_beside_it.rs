//! A theme can name font files to load, `- fonts { - plex_sans = "fonts/plex-sans.woff2" }`, and
//! the page asks for each with `load_overseer_font`. Both doors look for it in the folders themes
//! are kept in - the `themes` folder beside the settings, then the library's - and answer with its
//! type and its bytes in base64, from those folders and nowhere else.

use overseer::app_api::{font_in_themes, IconRefused, FONT_MOST_BYTES};
use overseer::server::{DocumentRoot, RequestError};
use serde_json::json;
use std::path::PathBuf;

/// The four bytes a WOFF2 file starts with, and a few after.
const WOFF2: &[u8] = &[b'w', b'O', b'F', b'2', 0, 1, 0, 0, 0xFF];
const WOFF2_BASE64: &str = "d09GMgABAAD/";

/// A root with its settings, a `themes` folder beside them holding fonts, and a font outside it
/// that may not be reached.
fn a_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("overseer_fonts_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("themes/fonts")).expect("make the root");
    std::fs::write(root.join("settings.os"), overseer::environment::DEFAULT).expect("write");
    std::fs::write(root.join("themes/fonts/plex-sans.woff2"), WOFF2).expect("write");
    std::fs::write(root.join("themes/fonts/Plex-Mono.WOFF"), WOFF2).expect("write");
    std::fs::write(root.join("themes/fonts/serif.ttf"), WOFF2).expect("write");
    std::fs::write(root.join("themes/fonts/display.otf"), WOFF2).expect("write");
    std::fs::write(root.join("themes/fonts/notes.txt"), "not a font").expect("write");
    std::fs::write(root.join("themes/fonts/huge.woff2"), vec![0u8; FONT_MOST_BYTES as usize + 1]).expect("write");
    std::fs::write(root.join("upstairs.woff2"), WOFF2).expect("write");
    root
}

fn ask(service: &DocumentRoot, font: &str) -> Result<serde_json::Value, RequestError> {
    service.command_for("alice", None, "load_overseer_font", &json!({ "font": font }))
}

#[test]
fn the_server_hands_out_a_font_kept_beside_its_themes_with_its_type() {
    let root = a_root("served");
    let service = DocumentRoot::new(&root).expect("open the root");
    assert_eq!(ask(&service, "fonts/plex-sans.woff2"), Ok(json!({ "mime": "font/woff2", "data": WOFF2_BASE64 })));
    assert_eq!(ask(&service, "fonts/Plex-Mono.WOFF").unwrap()["mime"], json!("font/woff"));
    assert_eq!(ask(&service, "fonts/serif.ttf").unwrap()["mime"], json!("font/ttf"));
    assert_eq!(ask(&service, "fonts/display.otf").unwrap()["mime"], json!("font/otf"));
}

#[test]
fn the_server_reads_no_font_outside_the_themes_folders() {
    let root = a_root("refused");
    let service = DocumentRoot::new(&root).expect("open the root");
    let outside = root.join("upstairs.woff2").to_string_lossy().replace('\\', "/");
    for font in [
        "../upstairs.woff2",
        "fonts/../../upstairs.woff2",
        "/upstairs.woff2",
        "C:/upstairs.woff2",
        outside.as_str(),
        "./fonts/plex-sans.woff2",
        "",
        "fonts/notes.txt",
        "fonts/huge.woff2",
        "../settings.os",
    ] {
        assert!(
            matches!(ask(&service, font), Err(RequestError::Rejected(_))),
            "'{}' was not refused: {:?}",
            font,
            ask(&service, font)
        );
    }
    assert!(matches!(ask(&service, "fonts/none.woff2"), Err(RequestError::NotFound(_))));
    assert!(matches!(
        service.command_for("alice", None, "load_overseer_font", &json!({})),
        Err(RequestError::Rejected(_))
    ));
}

#[test]
fn the_desktop_app_reads_fonts_from_beside_its_settings_as_the_server_does() {
    let root = a_root("desktop");
    overseer::environment::with_file(root.join("settings.os"), || {
        assert_eq!(font_in_themes("fonts/plex-sans.woff2"), Ok(json!({ "mime": "font/woff2", "data": WOFF2_BASE64 })));
        for font in ["../upstairs.woff2", "/upstairs.woff2", "fonts/notes.txt", "fonts/huge.woff2"] {
            assert!(matches!(font_in_themes(font), Err(IconRefused::NotAName(_))), "'{}' was not refused", font);
        }
        assert!(matches!(font_in_themes("fonts/none.woff2"), Err(IconRefused::Missing(_))));
    });
}

#[test]
fn the_themes_beside_the_settings_are_looked_in_before_the_library() {
    let root = a_root("order");
    overseer::environment::with_file(root.join("settings.os"), || {
        let folders = overseer::environment::theme_folders();
        assert_eq!(folders.first(), Some(&root.join("themes")));
        let library = overseer::bases::library().expect("the library is found").join("themes");
        assert_eq!(folders.last(), Some(&library));
        assert_eq!(folders.len(), 2);
    });
}
