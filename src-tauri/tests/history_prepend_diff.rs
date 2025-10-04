use overseer::file_ops::OverseerFileHandler;
use overseer::types::OverseerValue;

// Focused synthetic test: start with two similar list entries, prepend a new one via
// structured mutation, and ensure the original two keep their indentation after serialization.
#[test]
fn prepend_duplicate_list_entry_preserves_existing_indentation() {
    let original = r#"list History (entry=<Rec>) {
    - {
        - eid = 2
        - sets = 3
        - reps = 24
        - weight = 9.6
        - time = "2025-09-27T18:51:13.622996+00:00"
    }
    - {
        - eid = 7
        - sets = 3
        - reps = 22
        - weight = 16.1
        - time = "2025-09-27T14:46:03.651242600+00:00"
    }
}"#;

    let (_rem, mut nodes) = overseer::parser::parse_document(original).expect("parse");
    overseer::resolver::resolve_document(&mut nodes);

    let history_list = nodes
        .iter_mut()
        .find(|n| n.node_type == "list" && n.name == "History")
        .expect("History list");
    let template_entry = history_list
        .children
        .first()
        .expect("at least one entry")
        .clone();

    let mut new_entry = template_entry.clone();
    new_entry.source_snapshot = None;
    new_entry.source_fingerprint = None;
    new_entry.child_original_index = None;
    for child in new_entry.children.iter_mut() {
        child.source_snapshot = None;
        child.source_fingerprint = None;
        child.child_original_index = None;
        if child.name == "eid" {
            child
                .parameters
                .insert("value".to_string(), OverseerValue::Integer(1));
        } else if child.name == "time" {
            child.parameters.insert(
                "value".to_string(),
                OverseerValue::Timestamp("2099-12-31T23:59:59.000000+00:00".to_string()),
            );
        }
    }
    history_list.children.insert(0, new_entry);

    let regenerated = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

    fn extract_blocks(s: &str) -> Vec<Vec<&str>> {
        let mut blocks = Vec::new();
        let mut cur = Vec::new();
        let mut in_block = false;
        for line in s.lines() {
            let t = line.trim();
            if t == "- {" {
                in_block = true;
                cur.push(line);
            } else if in_block {
                cur.push(line);
                if t == "}" {
                    blocks.push(cur);
                    cur = Vec::new();
                    in_block = false;
                }
            }
        }
        blocks
    }

    let orig_blocks = extract_blocks(original);
    let regen_blocks = extract_blocks(&regenerated);
    assert_eq!(
        orig_blocks.len() + 1,
        regen_blocks.len(),
        "Expected exactly one new block added"
    );
    for (orig, regen_block) in orig_blocks.iter().zip(regen_blocks.iter().skip(1)) {
        for (ol, rl) in orig.iter().zip(regen_block.iter()) {
            let o_ws = ol.chars().take_while(|c| c.is_whitespace()).count();
            let r_ws = rl.chars().take_while(|c| c.is_whitespace()).count();
            assert_eq!(
                o_ws, r_ws,
                "Indentation changed for line:\nORIG: '{}'\nREGEN: '{}'\nRegenerated doc:\n{}",
                ol, rl, regenerated
            );
        }
    }
}
