use overseer::file_ops::OverseerFileHandler;
use overseer::parser;
use overseer::resolver;
use overseer::source_registry::SourceRegistry;
use std::cmp::max;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewlineKind {
    Lf,
    Crlf,
    Mixed,
    None,
}

#[derive(Debug, PartialEq, Eq)]
struct FixtureMetrics {
    comment_lines: usize,
    max_blank_run: usize,
    total_lines: usize,
    newline: NewlineKind,
    byte_len: usize,
}

fn collect_fixtures() -> Vec<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_root = manifest_dir.join("tests/fixtures/serializer");
    assert!(
        fixture_root.exists(),
        "Fixture directory missing: {:?}",
        fixture_root
    );
    let mut fixtures = Vec::new();
    let mut stack = vec![fixture_root];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir failed") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("os") {
                fixtures.push(path);
            }
        }
    }
    fixtures.sort();
    fixtures
}

fn analyze_fixture(text: &str) -> FixtureMetrics {
    let mut comment_lines = 0usize;
    let mut max_blank_run = 0usize;
    let mut blank_run = 0usize;
    let mut total_lines = 0usize;
    for line in text.lines() {
        total_lines += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_run += 1;
            max_blank_run = max(max_blank_run, blank_run);
        } else {
            blank_run = 0;
        }
        if trimmed.starts_with("//") {
            comment_lines += 1;
        }
    }
    let newline = detect_newline(text);
    FixtureMetrics {
        comment_lines,
        max_blank_run,
        total_lines,
        newline,
        byte_len: text.len(),
    }
}

fn detect_newline(text: &str) -> NewlineKind {
    let mut has_crlf = false;
    let mut has_bare_cr = false;
    let mut has_bare_lf = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if matches!(chars.peek(), Some('\n')) {
                    has_crlf = true;
                    chars.next();
                } else {
                    has_bare_cr = true;
                }
            }
            '\n' => {
                has_bare_lf = true;
            }
            _ => {}
        }
    }

    match (has_crlf, has_bare_cr, has_bare_lf) {
        (false, false, false) => NewlineKind::None,
        (true, false, false) => NewlineKind::Crlf,
        (false, false, true) => NewlineKind::Lf,
        (true, false, true) => NewlineKind::Mixed,
        (true, true, _) => NewlineKind::Mixed,
        (false, true, _) => NewlineKind::Mixed,
    }
}

fn first_diff(original: &str, regenerated: &str) -> String {
    let mismatch_idx = original
        .bytes()
        .zip(regenerated.bytes())
        .position(|(a, b)| a != b);
    match mismatch_idx {
        None => {
            if original.len() != regenerated.len() {
                format!(
                    "length mismatch: original {} bytes vs regenerated {} bytes",
                    original.len(),
                    regenerated.len()
                )
            } else {
                String::from("no mismatch detected")
            }
        }
        Some(idx) => {
            let orig_slice = &original.as_bytes()[idx..original.len().min(idx + 40)];
            let regen_slice = &regenerated.as_bytes()[idx..regenerated.len().min(idx + 40)];
            format!(
                "first mismatch at byte {}\n  original: {:?}\n  regen:    {:?}",
                idx, orig_slice, regen_slice
            )
        }
    }
}

fn dump_regenerated_fixture(name: &str, content: &str) {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dump_dir = manifest_dir.join("target/serializer-golden-failures");
    if fs::create_dir_all(&dump_dir).is_err() {
        eprintln!(
            "[serializer-golden] failed to create dump directory at {:?}",
            dump_dir
        );
        return;
    }
    let dump_path = dump_dir.join(name);
    if let Err(err) = fs::write(&dump_path, content) {
        eprintln!(
            "[serializer-golden] failed to write regenerated fixture {:?}: {}",
            dump_path, err
        );
    }
}

#[test]
fn serializer_round_trips_golden_fixtures() {
    let fixtures = collect_fixtures();
    assert!(
        !fixtures.is_empty(),
        "No serializer fixtures discovered under tests/fixtures/serializer"
    );

    for fixture in fixtures {
        let name = fixture
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>")
            .to_string();
        let source = fs::read_to_string(&fixture).expect("read fixture");
        assert!(
            !source.is_empty(),
            "Fixture {:?} is empty; tests require representative content",
            fixture
        );

        let expected_metrics = analyze_fixture(&source);

        let (remaining, mut nodes) = parser::parse_document(&source).expect("parse document");
        assert!(
            remaining.trim().is_empty(),
            "Fixture {} did not fully parse; remaining input: {:?}",
            name,
            remaining
        );
        resolver::resolve_document(&mut nodes);

        let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize nodes");
        let actual_metrics = analyze_fixture(&regenerated);

        if regenerated != source {
            dump_regenerated_fixture(&name, &regenerated);
            let diff = first_diff(&source, &regenerated);
            panic!(
                "Serializer output differed for fixture {}\noriginal metrics: {:?}\nregen metrics: {:?}\n{}",
                name, expected_metrics, actual_metrics, diff
            );
        }

        assert_eq!(
            actual_metrics, expected_metrics,
            "Serializer altered formatting metrics for fixture {}",
            name
        );

        // Reset registry state before proceeding to the next fixture to avoid cross-fixture bleed.
        SourceRegistry::reset();
    }
}
