use std::collections::{BTreeMap, BTreeSet};

use overseer::actions::ActionExecutor;
use overseer::parser;
use overseer::resolver;
use overseer::types::{OverseerNode, OverseerValue};

fn effective_name(node: &OverseerNode) -> String {
    if !node.name.is_empty() {
        node.name.clone()
    } else {
        node.node_type.clone()
    }
}

fn collect_done_paths(nodes: &[OverseerNode]) -> Vec<Vec<String>> {
    fn collect(nodes: &[OverseerNode], path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for node in nodes {
            let eff = effective_name(node);
            let ordinal = seen.entry(eff.clone()).or_insert(0);
            let segment = if *ordinal == 0 {
                eff.clone()
            } else {
                format!("{}#{}", eff, *ordinal)
            };
            *ordinal += 1;
            path.push(segment);
            if node.node_type == "button" && node.name == "done" {
                out.push(path.clone());
            }
            collect(&node.children, path, out);
            path.pop();
        }
    }

    let mut candidates = Vec::new();
    collect(nodes, &mut Vec::new(), &mut candidates);
    candidates
}

fn find_path_to_done_button(nodes: &[OverseerNode]) -> Option<Vec<String>> {
    let candidates = collect_done_paths(nodes);
    if let Some(path) = candidates
        .iter()
        .find(|p| p.iter().any(|seg| seg == "Exercises"))
    {
        return Some(path.clone());
    }
    candidates.into_iter().next()
}

fn history_node<'a>(nodes: &'a [OverseerNode]) -> Option<&'a OverseerNode> {
    for node in nodes {
        if node.node_type == "list" && node.name == "History" {
            return Some(node);
        }
        if let Some(found) = history_node(&node.children) {
            return Some(found);
        }
    }
    None
}

fn strip_history(nodes: &mut [OverseerNode]) {
    for node in nodes.iter_mut() {
        if node.node_type == "list" && node.name == "History" {
            node.children.clear();
        } else {
            strip_history(&mut node.children);
        }
    }
}

fn should_ignore_param(key: &str) -> bool {
    if key.starts_with("_template_") {
        return false;
    }
    if key == "value" {
        return false;
    }
    key.starts_with('_')
}

fn collect_differences(before: &[OverseerNode], after: &[OverseerNode]) -> Vec<String> {
    fn format_overseer_value(value: Option<&OverseerValue>) -> String {
        match value {
            Some(v) => format!("{:?}", v),
            None => "<none>".to_string(),
        }
    }

    fn visit(before: &OverseerNode, after: &OverseerNode, path: &mut Vec<String>, out: &mut Vec<String>) {
        path.push(effective_name(before));
        if before.node_type != after.node_type {
            out.push(format!(
                "{}: node_type {} -> {}",
                path.join("/"),
                before.node_type,
                after.node_type
            ));
        }

        let before_keys: BTreeSet<_> = before
            .parameters
            .keys()
            .filter(|k| !should_ignore_param(k.as_str()))
            .collect();
        let after_keys: BTreeSet<_> = after
            .parameters
            .keys()
            .filter(|k| !should_ignore_param(k.as_str()))
            .collect();
        for key in before_keys.union(&after_keys) {
            let b = before.parameters.get(*key);
            let a = after.parameters.get(*key);
            if b != a {
                let b_fmt = format_overseer_value(b);
                let a_fmt = format_overseer_value(a);
                out.push(format!(
                    "{}: param '{}' {} -> {}",
                    path.join("/"),
                    key,
                    b_fmt,
                    a_fmt
                ));
            }
        }

        if before.children.len() != after.children.len() {
            out.push(format!(
                "{}: children length {} -> {}",
                path.join("/"),
                before.children.len(),
                after.children.len()
            ));
        }

        let child_count = before.children.len().min(after.children.len());
        for idx in 0..child_count {
            visit(&before.children[idx], &after.children[idx], path, out);
        }

        path.pop();
    }

    let mut diffs = Vec::new();
    let count = before.len().min(after.len());
    for idx in 0..count {
        visit(&before[idx], &after[idx], &mut Vec::new(), &mut diffs);
    }
    if before.len() != after.len() {
        diffs.push(format!("root length {} -> {}", before.len(), after.len()));
    }
    diffs
}

#[test]
fn exercise_done_action_introduces_unexpected_mutations() {
    let content = include_str!("../../examples/exercise_tracker/exercise.os");
    let (_rest, mut nodes) = parser::parse_document(content).expect("parse exercise tracker");
    resolver::resolve_document(&mut nodes);

    let before = nodes.clone();
    let path = find_path_to_done_button(&nodes).expect("locate Done button path");

    ActionExecutor::execute_event(&mut nodes, &path, "click").expect("execute Done action");

    let history_before_len = history_node(&before).map(|n| n.children.len()).unwrap_or(0);
    let history_after_len = history_node(&nodes).map(|n| n.children.len()).unwrap_or(0);
    assert_eq!(history_before_len + 1, history_after_len, "One new history entry expected");

    let mut before_trim = before.clone();
    let mut after_trim = nodes.clone();
    strip_history(&mut before_trim);
    strip_history(&mut after_trim);

    let diffs = collect_differences(&before_trim, &after_trim);
    let unexpected: Vec<_> = diffs
        .iter()
        .filter(|entry| {
            !entry.starts_with("exercise_tracker/div/ExerciseRecordSample")
        })
        .cloned()
        .collect();
    assert!(
        unexpected.is_empty(),
        "Unexpected document changes beyond History detected: {:?}",
        unexpected
    );
    assert!(
        diffs
            .iter()
            .any(|entry| entry.starts_with("exercise_tracker/div/ExerciseRecordSample")),
        "Expected ExerciseRecordSample staging node to refresh with latest values"
    );
}
