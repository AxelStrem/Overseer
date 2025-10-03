#[cfg(test)]
mod tests {
    use crate::file_ops::OverseerFileHandler;
    use std::fs;
    use std::path::Path;

    // Helper: load .os examples recursively under ../../examples and run parse->resolve->serialize->merge
    #[test]
    fn merge_preserves_comment_counts_over_examples() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
        assert!(root.exists(), "examples directory missing: {:?}", root);
        let mut checked = 0usize;
        fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) { for entry in fs::read_dir(dir).unwrap() { let e = entry.unwrap(); let p = e.path(); if p.is_dir() { collect(&p, out); } else if p.extension().map(|s| s=="os").unwrap_or(false) { out.push(p); } } }
        let mut files = Vec::new(); collect(&root, &mut files); assert!(!files.is_empty(), "no .os example files found");
        for file in files {
            if let Ok(src) = fs::read_to_string(&file) {
                if src.trim().is_empty() {
                    continue;
                }

                let orig_comments: Vec<&str> = src
                    .lines()
                    .filter(|l| l.trim_start().starts_with("//"))
                    .collect();

                if let Ok((_rem, mut nodes)) = crate::parser::parse_document(&src) {
                    crate::resolver::resolve_document(&mut nodes);
                    if let Ok(regen) = OverseerFileHandler::serialize_nodes(&nodes) {
                        let regen_comments: Vec<&str> = regen
                            .lines()
                            .filter(|l| l.trim_start().starts_with("//"))
                            .collect();
                        assert_eq!(orig_comments.len(), regen_comments.len(), "Comment line count changed for {:?}", file);

                        let mut dup_ok = true;
                        let mut run = 0usize;
                        let mut last = "";
                        for c in &regen_comments {
                            if *c == last {
                                run += 1;
                            } else {
                                run = 1;
                                last = c;
                            }
                            if run > 2 {
                                dup_ok = false;
                                break;
                            }
                        }
                        assert!(dup_ok, "Detected >2 identical consecutive comment lines in {:?}", file);
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked>0, "No non-empty example .os files processed");
    }
}
