use std::fs;
use rand::{Rng, seq::SliceRandom};

// Randomized stress test to approximate manual interaction patterns that might induce sporadic indentation drift.
// Performs sequences of random mutations/removals followed by a prepend and asserts no indentation-only drift.
#[test]
fn history_randomized_stress_no_indentation_drift() {
    let path = std::path::Path::new("examples/exercise_tracker/exercise.os");
    let baseline = fs::read_to_string(path).expect("read baseline");
    let baseline_lines: Vec<&str> = baseline.lines().collect();

    // Utility: collapse a new merged file by removing the newly prepended block so we can compare unaffected lines.
    fn strip_new_block<'a>(merged_lines: &'a[&'a str], marker: &str) -> Vec<&'a str> {
        let mut start=None; let mut end=None; let mut depth=0isize;
        for (i,l) in merged_lines.iter().enumerate() { if l.contains(marker) { for j in (0..=i).rev() { if merged_lines[j].trim()=="- {" { start=Some(j); break; } }
            for k in i..merged_lines.len() { let t=merged_lines[k].trim(); if t=="- {" { depth+=1; } else if t=="}" { if depth==0 { end=Some(k); break; } else { depth-=1; } } }
            break; } }
        if let (Some(s),Some(e))=(start,end) { let mut out=Vec::new(); for (i,l) in merged_lines.iter().enumerate() { if i<s || i>e { out.push(*l); } } out } else { merged_lines.to_vec() }
    }

    // Pattern scan for indentation drift: a line whose trimmed content equals baseline counterpart but leading whitespace differs.
    fn scan_for_drift(orig:&[&str], merged:&[&str]) -> Option<(usize,String,String)> {
        let mut oi=0usize; let mut mi=0usize;
        while oi<orig.len() && mi<merged.len() { let o=orig[oi]; let m=merged[mi]; if o==m { oi+=1; mi+=1; continue; }
            if o.trim()==m.trim() { return Some((oi, o.to_string(), m.to_string())); }
            // allow for new or removed block already stripped; conservative advance
            oi+=1; mi+=1; }
        None
    }

    let iterations = 30; // adjustable
    let mut rng = rand::thread_rng();
    for iter in 0..iterations {
        // Parse fresh each iteration from baseline to avoid cumulative drift
        let (_rem, mut nodes) = crate::parser::parse_document(&baseline).expect("parse");
        crate::resolver::resolve_document(&mut nodes);

        // Locate exercise_tracker/History
        fn find_history<'a>(nodes: &'a mut [crate::ast::OverseerNode]) -> Option<&'a mut crate::ast::OverseerNode> { for n in nodes.iter_mut() { if n.name=="exercise_tracker" { for c in n.children.iter_mut() { if c.name=="History" { return Some(c);} } } } None }
        let hist = find_history(&mut nodes).expect("history");
        if hist.children.len()<3 { continue; }

        // Random mutations: choose up to 3 distinct indices (excluding first) and mutate reps/sets.
        let mut indices: Vec<usize> = (1..hist.children.len()).collect();
        indices.shuffle(&mut rng);
        for &ix in indices.iter().take(rng.gen_range(1..=3)) { if let Some(entry)=hist.children.get_mut(ix) {
            if let Some(v)=entry.parameters.get_mut("reps") { *v = crate::ast::OverseerValue::Int(rng.gen_range(10..500)); }
            if rng.gen_bool(0.5) { if let Some(v)=entry.parameters.get_mut("sets") { *v = crate::ast::OverseerValue::Int(rng.gen_range(1..10)); } }
        }}

        // Occasional removal of last entry (simulate user cleanup)
        if rng.gen_bool(0.3) { hist.children.pop(); }

        // Prepend new future timestamp entry
        let template_entry = hist.children.first().cloned().expect("one entry");
        let mut new_entry = template_entry.clone();
        let future_ts = format!("2099-12-31T23:59:{:02}.000000+00:00", iter % 60);
        if let Some(v)=new_entry.parameters.get_mut("time") { *v = crate::ast::OverseerValue::Timestamp(future_ts.clone()); }
        else { new_entry.parameters.insert("time".into(), crate::ast::OverseerValue::Timestamp(future_ts.clone())); }
        hist.children.insert(0, new_entry);

        let regenerated = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        let merged = crate::file_ops::OverseerFileHandler::merge_comments(&baseline, &regenerated);

        let merged_lines: Vec<&str> = merged.lines().collect();
        let stripped = strip_new_block(&merged_lines, &future_ts);
        if let Some((line_no, orig, new)) = scan_for_drift(&baseline_lines, &stripped) {
            panic!("Indentation drift detected at line {} in iteration {}\nORIG: {:?}\nNEW : {:?}", line_no+1, iter, orig, new);
        }
    }
}