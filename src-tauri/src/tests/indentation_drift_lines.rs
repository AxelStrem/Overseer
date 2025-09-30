use std::fs;

// Focused test to detect indentation drift on specific lines of interest after a prepend cycle.
#[test]
fn no_indentation_drift_on_selected_lines_after_prepend() {
    let path = std::path::Path::new("examples/exercise_tracker/exercise.os");
    let original = fs::read_to_string(path).expect("read example");

    // Parse + resolve original
    let (_rem, mut nodes) = crate::parser::parse_document(&original).expect("parse");
    crate::resolver::resolve_document(&mut nodes);

    // Locate History list
    fn find_history<'a>(nodes: &'a mut [crate::ast::OverseerNode]) -> Option<&'a mut crate::ast::OverseerNode> {
        for n in nodes.iter_mut() { if n.name == "exercise_tracker" { for c in n.children.iter_mut() { if c.name == "History" { return Some(c); } } } }
        None
    }
    let hist = find_history(&mut nodes).expect("history list");
    let template_entry = hist.children.first().expect("at least one entry").clone();

    // Clone with future timestamp
    let mut new_entry = template_entry.clone();
    if let Some(v) = new_entry.parameters.get_mut("time") { *v = crate::ast::OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".into()); }
    else { new_entry.parameters.insert("time".into(), crate::ast::OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".into())); }
    hist.children.insert(0, new_entry);

    let regenerated = crate::file_ops::OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
    let merged = crate::file_ops::OverseerFileHandler::merge_comments(&original, &regenerated);

    // Lines of interest (1-based indexing in human reference): 67, 114, 234, 235
    let original_lines: Vec<&str> = original.lines().collect();
    let merged_lines: Vec<&str> = merged.lines().collect();
    let targets = [67usize,114,234,235];
    for &ln in &targets {
        let oi = original_lines.get(ln-1).expect("orig line");
        let mi = merged_lines.get(ln-1).expect("merged line");
        // Compare leading whitespace + trimmed remainder separately for better diagnostics
        let orig_ws = oi.chars().take_while(|c| c.is_whitespace()).collect::<String>();
        let merged_ws = mi.chars().take_while(|c| c.is_whitespace()).collect::<String>();
        if orig_ws != merged_ws && oi.trim() == mi.trim() {
            panic!("Indentation drift on line {}: original indent {:?}, merged indent {:?}, content {:?}", ln, orig_ws, merged_ws, oi.trim());
        }
    }
}
