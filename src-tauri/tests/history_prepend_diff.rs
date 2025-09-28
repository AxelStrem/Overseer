// Focused synthetic test: start with two similar list entries, regenerate with a new
// earlier entry (duplicate anchor pattern) and ensure the original two keep indentation.
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

    // Simulate regenerated output that prepends a new entry (same internal field ordering) before originals.
    let regenerated = r#"list History (entry=<Rec>) {
    - {
        - eid = 1
        - sets = 1
        - reps = 10
        - weight = 5.0
        - time = "2099-12-31T23:59:59.000000+00:00"
    }
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

    let merged = overseer::file_ops::OverseerFileHandler::merge_comments(original, regenerated);

    // Extract the two original blocks from merged and compare indentation to original
    fn extract_blocks(s:&str)->Vec<Vec<&str>> {
        let mut blocks=Vec::new(); let mut cur=Vec::new(); let mut in_block=false; for line in s.lines() { let t=line.trim(); if t=="- {" { in_block=true; cur.push(line); } else if in_block { cur.push(line); if t=="}" { blocks.push(cur); cur=Vec::new(); in_block=false; } } }
        blocks
    }
    let orig_blocks = extract_blocks(original);
    let merged_blocks = extract_blocks(&merged);
    assert_eq!(orig_blocks.len()+1, merged_blocks.len(), "Expected exactly one new block added");
    // Compare block 1 & 2 of merged (skipping the prepended new block at index 0) to original 0 & 1
    for (orig, merged_block) in orig_blocks.iter().zip(merged_blocks.iter().skip(1)) {
        for (ol, ml) in orig.iter().zip(merged_block.iter()) {
            // Compare leading whitespace length
            let o_ws = ol.chars().take_while(|c| c.is_whitespace()).count();
            let m_ws = ml.chars().take_while(|c| c.is_whitespace()).count();
            assert_eq!(o_ws, m_ws, "Indentation changed for line:\nORIG: '{}'\nMERG: '{}'\nMerged doc:\n{}", ol, ml, merged);
        }
    }
}
