use overseer::file_ops::OverseerFileHandler;
use std::fs;
use std::path::Path;

#[test]
fn merge_preserves_comment_counts_over_examples() {
    // CARGO_MANIFEST_DIR points to src-tauri; examples live one directory up under ../examples.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    assert!(root.exists(), "examples directory missing: {:?}", root);
    let mut files = Vec::new();
    fn collect(dir:&Path, out:&mut Vec<std::path::PathBuf>) { for e in fs::read_dir(dir).unwrap() { let p=e.unwrap().path(); if p.is_dir() { collect(&p,out); } else if p.extension().map(|s| s=="os").unwrap_or(false) { out.push(p); } } }
    collect(&root,&mut files); assert!(!files.is_empty(), "no .os example files found");
    let mut checked=0usize;
    for file in files { let src = fs::read_to_string(&file).unwrap(); if src.trim().is_empty() { continue; }
        let orig_comments: Vec<&str> = src.lines().filter(|l| l.trim_start().starts_with("//")).collect();
        let Ok((_rem, mut nodes)) = overseer::parser::parse_document(&src) else { continue }; // skip parse failures silently
        overseer::resolver::resolve_document(&mut nodes);
    let regen = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    let regen_comments: Vec<&str> = regen.lines().filter(|l| l.trim_start().starts_with("//")).collect();
    assert_eq!(orig_comments.len(), regen_comments.len(), "Comment line count changed for {:?}", file);
    let mut run=0usize; let mut last=""; for c in &regen_comments { if *c==last { run+=1; } else { run=1; last=c; } assert!(run<=2, ">2 identical consecutive comment lines in {:?}", file); }
        checked+=1;
    }
    assert!(checked>0, "No qualifying example files processed");
}
