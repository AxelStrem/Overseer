use std::fs;

// Simulate a realistic user flow: mutate a mid-history entry's numeric field, then prepend a new entry.
// After merge, ensure all existing list field lines ("- <field> = ...") except those in the new block
// have identical indentation and content to original (aside from the deliberately mutated field value and new entry).
#[test]
fn mutation_then_prepend_preserves_other_indentation() {
    let path = std::path::Path::new("examples/exercise_tracker/exercise.os");
    let original = fs::read_to_string(path).expect("read example");

    // Parse + resolve original
    let (_rem, mut nodes) = crate::parser::parse_document(&original).expect("parse");
    crate::resolver::resolve_document(&mut nodes);

    // Find History list
    fn find_history<'a>(nodes: &'a mut [crate::ast::OverseerNode]) -> Option<&'a mut crate::ast::OverseerNode> {
        for n in nodes.iter_mut() { if n.name == "exercise_tracker" { for c in n.children.iter_mut() { if c.name == "History" { return Some(c); } } } }
        None
    }
    let hist = find_history(&mut nodes).expect("history list");
    assert!(hist.children.len() > 5, "expect several history entries");

    // Mutate a mid entry (e.g., index 5) changing reps value (simulate user edit)
    let mid_index = 5.min(hist.children.len()-1);
    if let Some(entry) = hist.children.get_mut(mid_index) {
        if let Some(val) = entry.parameters.get_mut("reps") { *val = crate::ast::OverseerValue::Int(999); }
        else { entry.parameters.insert("reps".into(), crate::ast::OverseerValue::Int(999)); }
    }

    // Prepend new future timestamp entry
    let template_entry = hist.children.first().cloned().expect("at least one entry");
    let mut new_entry = template_entry.clone();
    if let Some(v) = new_entry.parameters.get_mut("time") { *v = crate::ast::OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".into()); }
    else { new_entry.parameters.insert("time".into(), crate::ast::OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".into())); }
    hist.children.insert(0, new_entry);

    let regenerated = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    let merged = crate::file_ops::OverseerFileHandler::merge_comments(&original, &regenerated);

    let marker = "2099-12-31T23:59:59.000000+00:00";
    let merged_lines: Vec<&str> = merged.lines().collect();
    let mut new_block_range: Option<(usize,usize)> = None;
    // locate new block
    {
        let mut start = None; let mut end = None; let mut depth = 0isize;
        for (i,l) in merged_lines.iter().enumerate() { if l.contains(marker) { for j in (0..=i).rev() { if merged_lines[j].trim()=="- {" { start=Some(j); break; } }
                for k in i..merged_lines.len() { let t=merged_lines[k].trim(); if t=="- {" { depth+=1; } else if t=="}" { if depth==0 { end=Some(k); break; } else { depth-=1; } } }
                break; } }
        if let (Some(s),Some(e))=(start,end) { new_block_range=Some((s,e)); }
    }
    let mut original_lines: Vec<&str> = original.lines().collect();
    let mut merged_iter_index = 0usize; let mut orig_index = 0usize;

    let (new_s,new_e) = new_block_range.expect("new block range");
    // Build a quick lookup of mutated line (we know which entry index mutated); we allow that reps line to differ by value but not by indentation.
    let mutated_value_str = "- reps = 999"; // expected new content for mutated entry

    while merged_iter_index < merged_lines.len() && orig_index < original_lines.len() {
        if merged_iter_index >= new_s && merged_iter_index <= new_e { merged_iter_index+=1; continue; }
        let m = merged_lines[merged_iter_index];
        let o = original_lines[orig_index];
        // If this is the mutated reps line, skip content diff but verify indentation shape
        if m.trim()==mutated_value_str.trim() && o.trim().starts_with("- reps = ") {
            let m_ws = m.chars().take_while(|c| c.is_whitespace()).collect::<String>();
            let o_ws = o.chars().take_while(|c| c.is_whitespace()).collect::<String>();
            assert_eq!(m_ws, o_ws, "Indentation drift on mutated reps line");
            merged_iter_index+=1; orig_index+=1; continue;
        }
        if o != m {
            if o.trim() == m.trim() {
                panic!("Indentation drift (mutation flow) at original line {}\nORIG: {:?}\nNEW : {:?}", orig_index+1, o, m);
            } else {
                // Not equal and not a known change; report
                panic!("Unexpected content change at original line {}\nORIG: {:?}\nNEW : {:?}", orig_index+1, o, m);
            }
        }
        merged_iter_index+=1; orig_index+=1;
    }
}