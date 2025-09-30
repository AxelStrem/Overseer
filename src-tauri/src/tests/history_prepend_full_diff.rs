use std::fs;

// Comprehensive regression test: after prepending a synthetic future History entry,
// every other line (content + indentation) should match the original exactly.
// This is stricter than the earlier test and will catch subtle indentation drift
// (e.g., an extra single leading space on a line like '- reps = 20').
#[test]
fn history_prepend_full_diff_no_indentation_or_content_drift() {
    let path = std::path::Path::new("examples/exercise_tracker/exercise.os");
    let original = fs::read_to_string(path).expect("read example");

    // Parse + resolve original document
    let (_rem, mut nodes) = crate::parser::parse_document(&original).expect("parse");
    crate::resolver::resolve_document(&mut nodes);

    // Find History list
    fn find_history<'a>(nodes: &'a mut [crate::ast::OverseerNode]) -> Option<&'a mut crate::ast::OverseerNode> {
        for n in nodes.iter_mut() { if n.name == "exercise_tracker" { for c in n.children.iter_mut() { if c.name == "History" { return Some(c); } } } }
        None
    }
    let hist = find_history(&mut nodes).expect("history list");
    let template_entry = hist.children.first().cloned().expect("at least one history entry");

    // Create future timestamp entry
    let mut new_entry = template_entry.clone();
    if let Some(v) = new_entry.parameters.get_mut("time") { *v = crate::ast::OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".into()); }
    else { new_entry.parameters.insert("time".into(), crate::ast::OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".into())); }
    hist.children.insert(0, new_entry);

    let regenerated = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    let merged = crate::file_ops::OverseerFileHandler::merge_comments(&original, &regenerated);

    // Locate new block boundaries by timestamp marker
    let marker = "2099-12-31T23:59:59.000000+00:00";
    let merged_lines: Vec<&str> = merged.lines().collect();
    let mut start_idx = None; let mut end_idx = None; let mut depth = 0usize;
    for (i, line) in merged_lines.iter().enumerate() {
        if line.contains(marker) {
            for j in (0..=i).rev() { if merged_lines[j].trim() == "- {" { start_idx = Some(j); break; } }
            for k in i..merged_lines.len() {
                let t = merged_lines[k].trim();
                if t == "- {" { depth += 1; }
                else if t == "}" { if depth == 0 { end_idx = Some(k); break; } else { depth -= 1; } }
            }
            break;
        }
    }
    let (s,e) = (start_idx.expect("start"), end_idx.expect("end"));

    // Compare originals vs merged excluding the new block.
    let original_lines: Vec<&str> = original.lines().collect();
    let mut o_i = 0usize; // index into original lines
    let mut first_diff: Option<(usize,String,String)> = None;
    for (m_i, m_line) in merged_lines.iter().enumerate() {
        if m_i >= s && m_i <= e { continue; } // skip new block lines
        if o_i >= original_lines.len() { break; }
        let o_line = original_lines[o_i];
        if o_line != *m_line {
            // Offer shorthand classification: same trimmed? indentation drift.
            if o_line.trim() == m_line.trim() {
                first_diff = Some((o_i, format!("INDENT: {:?}", o_line), format!("INDENT: {:?}", m_line)));
            } else {
                first_diff = Some((o_i, o_line.to_string(), m_line.to_string()));
            }
            break;
        }
        o_i += 1;
    }
    if let Some((line_no, orig, new)) = first_diff { panic!("Drift after prepend at original line {}\nORIG: {}\nNEW : {}", line_no+1, orig, new); }
}