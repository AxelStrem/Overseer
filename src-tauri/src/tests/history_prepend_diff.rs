use std::fs;

// Integration-style test verifying that prepending a new history entry to the large
// exercise tracker example only introduces that new block in the merged output
// (no incidental indentation or whitespace changes to existing history entries).
#[test]
fn prepending_history_entry_only_adds_new_block() {
    let path = std::path::Path::new("examples/exercise_tracker/exercise.os");
    let original = fs::read_to_string(path).expect("read example");

    // Parse + resolve original
    let (_rem, mut nodes) = crate::parser::parse_document(&original).expect("parse");
    crate::resolver::resolve_document(&mut nodes);

    // Find History list and clone first entry to simulate a new one with current timestamp
    fn find_history<'a>(nodes: &'a mut [crate::ast::OverseerNode]) -> Option<&'a mut crate::ast::OverseerNode> {
        for n in nodes.iter_mut() {
            if n.name == "exercise_tracker" { // root div
                for c in n.children.iter_mut() {
                    if c.name == "History" { return Some(c); }
                }
            }
        }
        None
    }
    let hist = find_history(&mut nodes).expect("history list");
    // entries are children with name implicitly representing the list entry (anonymous block)
    let template_entry = hist.children.iter().find(|c| c.name.is_empty() || c.name == "{entry}").or_else(|| hist.children.first()).expect("at least one entry").clone();

    // Create new entry with a timestamp guaranteed to sort to the top (future time)
    let mut new_entry = template_entry.clone();
    // update time parameter; find param 'time'
    if let Some(v) = new_entry.parameters.get_mut("time") { *v = crate::ast::OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".to_string()); }
    else { new_entry.parameters.insert("time".into(), crate::ast::OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".to_string())); }

    // Prepend by inserting at beginning of children vec
    hist.children.insert(0, new_entry);

    // Serialize regenerated canonical
    let regenerated = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

    // Merge comments
    let merged = crate::file_ops::OverseerFileHandler::merge_comments(&original, &regenerated);

    // Diff logic: ensure original is contained in merged (minus the new first block) with identical indentation
    // Strategy: locate the new timestamp line and remove its surrounding entry block from merged, then compare remaining
    let marker = "2099-12-31T23:59:59.000000+00:00";
    let merged_lines: Vec<&str> = merged.lines().collect();
    let mut start_idx = None; let mut end_idx = None; let mut depth = 0usize;
    for (i, line) in merged_lines.iter().enumerate() {
        if line.contains(marker) { // inside the new block; walk backward to its opening "- {"
            // backward search for line containing "- {"
            for j in (0..=i).rev() {
                if merged_lines[j].trim() == "- {" { start_idx = Some(j); break; }
            }
            // forward search to closing single '}' that aligns one indent level
            for k in i..merged_lines.len() {
                let t = merged_lines[k].trim();
                if t == "- {" { depth += 1; }
                else if t == "}" {
                    if depth == 0 { end_idx = Some(k); break; } else { depth -=1; }
                }
            }
            break;
        }
    }
    let (s,e) = (start_idx.expect("start"), end_idx.expect("end"));
    let mut without_new = Vec::new();
    for (i, l) in merged_lines.iter().enumerate() { if i < s || i > e { without_new.push(*l); } }
    let reconstructed = without_new.join("\n") + "\n"; // ensure trailing newline

    // Normalize repeated blank lines for a lenient comparison (should already be stable)
    fn collapse_blank_runs(s: &str) -> String { let mut out = String::new(); let mut run=0; for ch in s.chars() { if ch=='\n' { run+=1; if run<=2 { out.push(ch); } } else { run=0; out.push(ch);} } out }
    let orig_norm = collapse_blank_runs(&original);
    let recon_norm = collapse_blank_runs(&reconstructed);

    if orig_norm != recon_norm {
        // Provide a focused diff around first mismatch
        let mut mismatch_report = String::new();
        let o_lines: Vec<&str> = orig_norm.lines().collect();
        let r_lines: Vec<&str> = recon_norm.lines().collect();
        let mut first_diff = None;
        for i in 0..o_lines.len().min(r_lines.len()) { if o_lines[i] != r_lines[i] { first_diff = Some(i); break; } }
        if let Some(idx) = first_diff { let start = idx.saturating_sub(5); let end = (idx+5).min(o_lines.len()); for i in start..end { mismatch_report.push_str(&format!("ORIG {:04}: {}\n", i, o_lines[i])); mismatch_report.push_str(&format!("NEW  {:04}: {}\n", i, r_lines[i])); } }
        panic!("Indentation/whitespace drift detected after merge when prepending new history entry. First diff vicinity:\n{}", mismatch_report);
    }
}
