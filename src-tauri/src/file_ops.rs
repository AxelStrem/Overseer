use std::collections::VecDeque;
use std::path::Path;
use tokio::fs;

use crate::source_registry::SourceRegistry;
use crate::types::*;

#[allow(dead_code)]
pub struct FileOperations;

struct FormattingPreferences {
    indent_unit: String,
    newline: String,
}

#[allow(dead_code)]
impl FileOperations {
    pub async fn read_file(path: &str) -> Result<String> {
        match fs::read_to_string(path).await {
            Ok(content) => Ok(content),
            Err(e) => Err(OverseerError::IoError(format!(
                "Failed to read file '{}': {}",
                path, e
            ))),
        }
    }

    pub async fn write_file(path: &str, content: &str) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(path).parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return Err(OverseerError::IoError(format!(
                    "Failed to create directories for '{}': {}",
                    path, e
                )));
            }
        }

        match fs::write(path, content).await {
            Ok(_) => Ok(()),
            Err(e) => Err(OverseerError::IoError(format!(
                "Failed to write file '{}': {}",
                path, e
            ))),
        }
    }

    pub async fn file_exists(path: &str) -> bool {
        Path::new(path).exists()
    }

    pub async fn create_backup(path: &str) -> Result<String> {
        let backup_path = format!("{}.backup", path);

        if Self::file_exists(path).await {
            match fs::copy(path, &backup_path).await {
                Ok(_) => Ok(backup_path),
                Err(e) => Err(OverseerError::IoError(format!(
                    "Failed to create backup of '{}': {}",
                    path, e
                ))),
            }
        } else {
            Err(OverseerError::IoError(format!(
                "Cannot backup file '{}': file does not exist",
                path
            )))
        }
    }

    pub fn validate_file_path(path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(OverseerError::IoError(
                "File path cannot be empty".to_string(),
            ));
        }

        // Check for invalid characters (Windows specific)
        let invalid_chars = ['<', '>', ':', '"', '|', '?', '*'];
        if path.chars().any(|c| invalid_chars.contains(&c)) {
            return Err(OverseerError::IoError(format!(
                "File path contains invalid characters: {}",
                path
            )));
        }

        // Check if path is too long (Windows limit is 260 characters)
        if path.len() > 260 {
            return Err(OverseerError::IoError("File path is too long".to_string()));
        }

        Ok(())
    }

    pub async fn save_overseer_file(path: &str, nodes: &[OverseerNode]) -> Result<()> {
        FileOperations::validate_file_path(path)?;

        // Create backup before saving
        if FileOperations::file_exists(path).await {
            FileOperations::create_backup(path).await?;
        }

        let content = Self::serialize_nodes(nodes)?;
        FileOperations::write_file(path, &content).await
    }

    /// Trailing trivia of the document that owns these nodes, identified by the first
    /// source id found among the roots (or, failing that, anywhere in the tree - a document
    /// whose roots were all rebuilt still has original descendants).
    fn document_trailing_for(nodes: &[OverseerNode]) -> String {
        fn first_id(nodes: &[OverseerNode]) -> Option<&str> {
            for n in nodes {
                if let Some(id) = n.source_id.as_deref() {
                    return Some(id);
                }
            }
            for n in nodes {
                if let Some(id) = first_id(&n.children) {
                    return Some(id);
                }
            }
            None
        }
        first_id(nodes)
            .map(SourceRegistry::document_trailing_for_id)
            .unwrap_or_default()
    }

    pub fn serialize_nodes(nodes: &[OverseerNode]) -> Result<String> {
        // A list entry the template gave is the template's text, not this document's: taken out
        // before anything is written, so that no way of writing a list can put it in the file.
        // Nor a context's copy, which the resolver works out from the declarations the file holds.
        let without_given;
        let nodes = if Self::holds_a_given_entry(nodes) || crate::contexts::holds_a_copy(nodes) {
            let mut copy = nodes.to_vec();
            Self::leave_out_given_entries(&mut copy);
            crate::contexts::leave_out_copies(&mut copy);
            without_given = copy;
            without_given.as_slice()
        } else {
            nodes
        };
        let mut output = String::new();
        #[cfg(test)]
        let _registry_guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let prefs = Self::detect_formatting(nodes);
        let indent_fallback = prefs.indent_unit.clone();

        for node in nodes {
            Self::serialize_node(node, &mut output, 0, indent_fallback.as_str())?;
        }

        // Trailing trivia belongs to the document these nodes came from. Taking it from a
        // process-wide slot appended whichever document was parsed most recently - for a
        // host with a mount, that is the mounted file, whose closing comments then landed
        // at the end of the host.
        let trailing = Self::document_trailing_for(nodes);
        if !trailing.is_empty() {
            Self::push_trivia(&mut output, &trailing);
        }

        if prefs.newline == "\r\n" {
            // Strip every CR first: snapshot replay emits its source text verbatim, so parts of
            // `output` already carry CRLF. Expanding every '\n' without stripping would turn
            // those into "\r\r\n" - a stray CR injected on every replayed line, compounding on
            // each save. Collapsing only "\r\n" is not enough either, since an already-doubled
            // "\r\r\n" would survive as "\r\n" and be re-expanded. Removing all CR makes this
            // idempotent regardless of how a given line was produced.
            output = output.replace('\r', "").replace('\n', "\r\n");
        }

        Ok(output)
    }

    fn holds_a_given_entry(nodes: &[OverseerNode]) -> bool {
        nodes.iter().any(|node| {
            crate::resolver::is_a_given_entry(node) || Self::holds_a_given_entry(&node.children)
        })
    }

    fn leave_out_given_entries(nodes: &mut Vec<OverseerNode>) {
        nodes.retain(|node| !crate::resolver::is_a_given_entry(node));
        for node in nodes.iter_mut() {
            Self::leave_out_given_entries(&mut node.children);
        }
    }

    /// Whether a marker says the *node* came from a template.
    ///
    /// Every `_template_*` marker is left by a template, on what it made - a parameter it gave
    /// (`_template_label`), or the node itself (`_template_node`). Styles used to be copied from a
    /// container onto everything inside it with the same marker, and a group inside a coloured
    /// template read as wholly the template's: the meal card's twenty-three lines were written
    /// back as `div (layout="horizontal", margin=0) {}`. Nothing is copied that way now, so the
    /// marker means what it says again.
    fn marks_a_template_node(key: &str) -> bool {
        key.starts_with("_template_")
    }

    fn push_trivia(output: &mut String, trivia: &str) {
        if trivia.is_empty() {
            return;
        }
        let mut normalized = if trivia.contains("\r\n") {
            trivia.replace("\r\n", "\n")
        } else {
            trivia.to_string()
        };
        if normalized.is_empty() {
            return;
        }

        if normalized.chars().all(|c| c == '\n') {
            let existing_blank_run = output.chars().rev().take_while(|&ch| ch == '\n').count();
            let desired_blank_run = normalized.len();
            if desired_blank_run <= existing_blank_run {
                return;
            }
            let additional = desired_blank_run - existing_blank_run;
            normalized = "\n".repeat(additional);
        } else if output.ends_with('\n')
            && normalized.starts_with('\n')
            && normalized.trim().is_empty()
        {
            normalized.remove(0);
        }

        output.push_str(&normalized);
    }

    fn emit_leading_trivia(
        output: &mut String,
        trivia: &str,
        node: &OverseerNode,
        allow: bool,
        indent_level: usize,
        in_list_body: bool,
    ) -> bool {
        if !allow || trivia.is_empty() {
            return false;
        }

        // The block before this node can leave behind an indent, a line begun and not yet written
        // on, or the whole gap up to this node. Trivia that starts a line of its own makes that
        // indent a line holding nothing but spaces, which the next parse keeps; and trivia
        // holding only whitespace is that gap, written a second time. Each save added to it.
        if trivia.starts_with(['\n', '\r']) {
            if trivia.trim().is_empty() {
                Self::drop_trailing_whitespace_line(output);
            } else {
                let kept = output.trim_end_matches([' ', '\t']).len();
                if kept < output.len() && output[..kept].ends_with('\n') {
                    output.truncate(kept);
                }
            }
        }

        let mut allowed_blank_lines = node.leading_blank_lines as usize;
        let trivia_is_whitespace = trivia.trim().is_empty();
        if in_list_body {
            allowed_blank_lines = 0;
        } else if output.is_empty() && !trivia_is_whitespace {
            // Nothing has been written yet and the trivia is the top of the file. Its own text
            // already holds whatever blank lines separate the comment blocks in it, so a count
            // of them emitted in front is that spacing a second time - and since every save
            // rewrites the file, the blank lines accumulated one per save. Three blocks were
            // enough to start it, which is why a two-paragraph header never showed it and a
            // three-paragraph one did.
            allowed_blank_lines = 0;
        } else if !output.is_empty() {
            // At the top of a document the gap between two nodes is held twice: at the end of
            // the text of the node before, and here. Counting only the first lost the blank line
            // whenever the node before was rebuilt rather than given back, since a rebuilt
            // container ends at its brace. Counted here too, the newlines already written are
            // taken off below, so a node given back as read gets nothing added.
            if indent_level == 0 && !trivia_is_whitespace {
                allowed_blank_lines = 0;
            } else if trivia_is_whitespace {
                allowed_blank_lines = allowed_blank_lines.saturating_sub(1);
            }
        }

        let leading = trivia.to_string();
        let existing_run = output.chars().rev().take_while(|&ch| ch == '\n').count();
        let desired_total_newlines = if output.is_empty() {
            allowed_blank_lines
        } else {
            allowed_blank_lines + 1
        };
        let mut prefix_bytes = 0;
        for ch in leading.chars() {
            match ch {
                '\n' | '\r' => {
                    prefix_bytes += ch.len_utf8();
                }
                _ => break,
            }
        }

        let mut rest = leading[prefix_bytes..].replace("\r\n", "\n");
        // A node made from a template carries the template's trivia, and the indent at its end
        // is how deep the template sits, not the node. A list in an entry happened to agree with
        // its template one level down; a list in an entry of that list was written at the
        // template's depth. Left off, the line is indented the way any node without text of its
        // own is - by how deep it actually is.
        let made_from_a_template = node.source_snapshot.as_ref().is_some_and(|snap| {
            matches!(snap.origin, SnapshotOrigin::Synthetic(SyntheticSnapshotKind::TemplateClone))
        });
        if made_from_a_template {
            let kept = rest.trim_end_matches([' ', '\t']).len();
            rest.truncate(kept);
        }

        // Whitespace trivia keeps the lines it holds past its first newline, and those are blank
        // lines already counted among the node's. Padding the full count in front of them as well
        // wrote each of them twice, and the next parse counted them all again.
        let held_in_rest = if trivia_is_whitespace {
            rest.matches('\n').count()
        } else {
            0
        };
        let allowed_prefix =
            desired_total_newlines.saturating_sub(existing_run + held_in_rest);

        for _ in 0..allowed_prefix {
            output.push('\n');
        }

        Self::push_trivia(output, &rest);
        true
    }

    /// The value a node carries because something wrote it, rather than because its template
    /// said so.
    ///
    /// Only an explicit override counts. "Differs from what the template said" would also catch
    /// a formula whose worked-out value naturally differs from the formula it was written as,
    /// and writing that into an entry would freeze a computed number into the document.
    fn value_written_into(node: &OverseerNode) -> Option<&OverseerValue> {
        if !matches!(
            node.parameters.get("_explicit_child_override"),
            Some(OverseerValue::Boolean(true))
        ) {
            return None;
        }
        let value = node.parameters.get("value")?;
        // A node carrying parameters of its own needs more than `- name = value` to describe it,
        // and that is a shape this does not attempt.
        let says_more = node.parameters.iter().any(|(k, _)| {
            let key = k.as_str();
            !key.starts_with('_')
                && key != "value"
                && !node.parameters.contains_key(&format!("_template_{}", key))
        });
        if says_more || !node.children.is_empty() {
            return None;
        }
        Some(value)
    }

    /// Write what was written inside `node` as the body of an entry override, and say whether
    /// there was anything at all.
    ///
    /// Leaves come out as `- name = value`. A container that stands for itself comes out as a
    /// block naming it, holding the same again - the form an entry already uses for a nested
    /// list. Transparent wrappers are looked straight through, because an address does not name
    /// them and the entry writes their contents as its own. A list something was written into
    /// comes out whole, at `level`, the depth `indent` stands for.
    fn write_overrides_inside(
        node: &OverseerNode,
        output: &mut String,
        indent: &str,
        level: usize,
        unit: &str,
    ) -> Result<bool> {
        let mut wrote = false;
        for child in &node.children {
            if crate::addressing::is_wrapper(child) {
                if Self::write_overrides_inside(child, output, indent, level, unit)? {
                    wrote = true;
                }
                continue;
            }
            if Self::is_a_written_list(child) {
                // Written after a line of its own, as it will stand: into an empty buffer, the
                // spacing rules take it for the top of a file and give it blank lines of its own.
                let mut list = String::from("\n");
                Self::serialize_node_context(child, &mut list, level, false, unit, None, false)?;
                output.push_str(&list[1..]);
                wrote = true;
                continue;
            }
            if child.children.is_empty() {
                if let Some(value) = Self::value_written_into(child) {
                    output.push_str(&format!(
                        "{}- {} = {}\n",
                        indent,
                        child.name,
                        Self::serialize_value_with_node(child, value)
                    ));
                    wrote = true;
                }
                continue;
            }
            // Only into a plain container. A node standing for an instance of some other
            // template - `NutriScore quality` inside a meal record - holds its template's own
            // body, and what is in there belongs to that template rather than to this entry.
            // Walking into one copies its formulas into every record that uses it, which says
            // something the entry never said and could not have meant.
            if child.node_type != "div" {
                continue;
            }
            let mut inside = String::new();
            let deeper = format!("{}{}", indent, unit);
            if Self::write_overrides_inside(child, &mut inside, &deeper, level + 1, unit)? {
                output.push_str(&format!("{}{} {} {{\n", indent, child.node_type, child.name));
                output.push_str(&inside);
                output.push_str(&format!("{}}}\n", indent));
                wrote = true;
            }
        }
        Ok(wrote)
    }

    /// A list an entry has written into: it holds records of the entry's own, which no template
    /// supplies, so it is written whole wherever it sits.
    fn is_a_written_list(node: &OverseerNode) -> bool {
        node.node_type == "list"
            && matches!(
                node.parameters.get("_explicit_child_override"),
                Some(OverseerValue::Boolean(true))
            )
    }

    /// Whether a wrapper holds such a list, looking through the wrappers inside it.
    fn wrapper_holds_a_written_list(node: &OverseerNode) -> bool {
        node.children.iter().any(|c| {
            Self::is_a_written_list(c)
                || (crate::addressing::is_wrapper(c) && Self::wrapper_holds_a_written_list(c))
        })
    }

    fn drop_trailing_whitespace_line(buffer: &mut String) {
        loop {
            if buffer.is_empty() {
                return;
            }

            let mut end = buffer.len();
            while end > 0 && buffer.as_bytes()[end - 1] == b'\n' {
                end -= 1;
            }

            if end == 0 {
                buffer.clear();
                return;
            }

            let line_start = buffer[..end]
                .rfind('\n')
                .map(|pos| pos + 1)
                .unwrap_or(0);
            let line = &buffer[line_start..end];

            if line.trim().is_empty() {
                buffer.truncate(line_start);
                continue;
            }

            break;
        }
    }

    fn emit_trailing_trivia(snapshot: &Option<std::sync::Arc<NodeSourceSnapshot>>, output: &mut String) {
        if let Some(snap) = snapshot {
            if snap.trailing_trivia.trim().is_empty() {
                return;
            }
            Self::push_trivia(output, &snap.trailing_trivia);
        }
    }

    fn should_skip_trailing_trivia(node: &OverseerNode) -> bool {
        let from_template = node.parameters.get("_from_template");
        matches!(from_template, Some(OverseerValue::Boolean(true)))
    }

    fn emit_trailing_trivia_if_allowed(
        node: &OverseerNode,
        snapshot: &Option<std::sync::Arc<NodeSourceSnapshot>>,
        output: &mut String,
    ) {
        if Self::should_skip_trailing_trivia(node) {
            return;
        }
        Self::emit_trailing_trivia(snapshot, output);
    }

    /// The node's block exactly as authored, braces included.
    ///
    /// `snapshot_block_inner` deliberately returns the text *between* the braces, and gives up
    /// when there is nothing between them - which is every empty block, `{ }` and `{\n}` alike.
    /// The childless branch of the emitter then had nothing to write and produced a bodiless
    /// declaration, quietly dropping the braces from any empty block it reformatted.
    fn snapshot_block_verbatim(snapshot: &NodeSourceSnapshot) -> Option<String> {
        let open = snapshot.body.child_envelope.open.as_ref()?;
        let close = snapshot.body.child_envelope.close.as_ref()?;
        let span_start = snapshot.span.0;
        let rel_start = open.span.0.checked_sub(span_start)?;
        let rel_end = close.span.1.checked_sub(span_start)?;
        if rel_start >= rel_end || rel_end > snapshot.full_text.len() {
            return None;
        }
        if !snapshot.full_text.is_char_boundary(rel_start)
            || !snapshot.full_text.is_char_boundary(rel_end)
        {
            return None;
        }
        Some(snapshot.full_text[rel_start..rel_end].replace("\r\n", "\n"))
    }

    /// Whether a block, braces and all, holds nothing but whitespace and comments.
    fn holds_only_trivia(block: &str) -> bool {
        let inner = block.trim();
        let inner = inner.strip_prefix('{').unwrap_or(inner);
        let inner = inner.strip_suffix('}').unwrap_or(inner);
        let mut rest = inner.to_string();
        while let Some(open) = rest.find("/*") {
            match rest[open..].find("*/") {
                Some(close) => rest.replace_range(open..open + close + 2, " "),
                None => rest.truncate(open),
            }
        }
        rest.lines().all(|line| {
            let line = match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            };
            line.trim().is_empty()
        })
    }

    fn snapshot_block_inner(snapshot: &NodeSourceSnapshot) -> Option<String> {
        let (body_start, body_end) = snapshot.body_span?;
        if body_end <= body_start {
            return None;
        }
        let span_start = snapshot.span.0;
        let rel_start = body_start.saturating_sub(span_start);
        let rel_end = body_end.saturating_sub(span_start);
        if rel_start >= snapshot.full_text.len()
            || rel_end > snapshot.full_text.len()
            || rel_start >= rel_end
        {
            return None;
        }
        let slice = &snapshot.full_text[rel_start..rel_end];
        let first_newline = slice.find('\n')?;
        let last_newline = slice.rfind('\n')?;
        if last_newline <= first_newline {
            return None;
        }
        let inner = &slice[first_newline + 1..last_newline + 1];
        if inner.trim().is_empty() {
            return None;
        }
        Some(inner.replace("\r\n", "\n"))
    }

    fn is_auto_generated_entry_name(name: &str) -> bool {
        let trimmed = name.trim();
        if let Some((prefix, suffix)) = trimmed.rsplit_once("__") {
            if prefix.is_empty() {
                return false;
            }
            return suffix.chars().all(|c| c.is_ascii_digit());
        }
        false
    }

    fn snapshot_for(node: &OverseerNode) -> Option<std::sync::Arc<NodeSourceSnapshot>> {
        if let Some(snapshot) = node.source_snapshot.clone() {
            return Some(snapshot);
        }
        let found = node.source_id.as_deref().and_then(SourceRegistry::get);

        // A node that carries an id but whose snapshot has gone is about to be reformatted:
        // it loses its braces, comments, indentation and parameter order. That is invisible
        // in the output - the document simply comes back slightly different - so make it
        // observable on demand. Set OVERSEER_DEBUG_SNAPSHOTS=1 to report each miss.
        if found.is_none() {
            if let Some(id) = node.source_id.as_deref() {
                if std::env::var("OVERSEER_DEBUG_SNAPSHOTS").is_ok() {
                    eprintln!(
                        "[SNAPSHOT MISS] id={} node={} type={} children={} - will be reformatted",
                        id,
                        if node.name.is_empty() { "(unnamed)" } else { &node.name },
                        node.node_type,
                        node.children.len()
                    );
                }
            }
        }
        found
    }

    fn detect_formatting(nodes: &[OverseerNode]) -> FormattingPreferences {
        fn visit(
            nodes: &[OverseerNode],
            indent: &mut Option<String>,
            newline: &mut Option<String>,
        ) {
            for node in nodes {
                if indent.is_some() && newline.is_some() {
                    return;
                }
                if let Some(snapshot) = FileOperations::snapshot_for(node) {
                    if indent.is_none() {
                        if let Some(ind) = snapshot.indent_unit.clone() {
                            if !ind.is_empty() {
                                *indent = Some(ind);
                            }
                        }
                    }
                    if newline.is_none() {
                        if let Some(nl) = snapshot.newline.clone() {
                            if !nl.is_empty() {
                                *newline = Some(nl);
                            }
                        }
                    }
                }
                if indent.is_some() && newline.is_some() {
                    return;
                }
                if !node.children.is_empty() {
                    visit(&node.children, indent, newline);
                }
            }
        }

        let mut indent = None;
        let mut newline = None;
        visit(nodes, &mut indent, &mut newline);

        FormattingPreferences {
            indent_unit: indent.unwrap_or_else(|| "    ".to_string()),
            newline: newline.unwrap_or_else(|| "\n".to_string()),
        }
    }

    fn serialize_node(
        node: &OverseerNode,
        output: &mut String,
        indent_level: usize,
        indent_unit: &str,
    ) -> Result<()> {
        Self::serialize_node_context(
            node,
            output,
            indent_level,
            false,
            indent_unit,
            None,
            false,
        )
    }

    fn serialize_node_context(
        node: &OverseerNode,
        output: &mut String,
        indent_level: usize,
        in_list_body: bool,
        fallback_indent_unit: &str,
        list_indent_hint: Option<usize>,
        force_emit_template_children: bool,
    ) -> Result<()> {
        // A mount's children belong to another file. Loading one fills them in, which changes
        // the node's fingerprint, so the snapshot fast path stops replaying the authored
        // `{ }` and the fallback formatter writes the whole mounted document into the host.
        // Serializing the declaration with an empty body states the invariant directly:
        // whatever a mount is currently holding, none of it is this document's text.
        // An entry the renderer materialized by cloning a template carries the template's
        // whole structure, because the click that created it addresses a node inside the
        // entry and that path has to resolve. None of that belongs on disk: the template
        // already supplies it, and writing it out turns a two-line entry into a transcription
        // of the template, comments included. Record only what distinguishes this entry -
        // fields explicitly overridden (its key, and anything since edited) and lists that
        // have gained records.
        if matches!(
            node.parameters.get("_materialized_from_template"),
            Some(OverseerValue::Boolean(true))
        ) {
            let keeps_a_value = |c: &OverseerNode| {
                matches!(
                    c.parameters.get("_override_present"),
                    Some(OverseerValue::Boolean(true))
                ) || matches!(
                    c.parameters.get("_explicit_child_override"),
                    Some(OverseerValue::Boolean(true))
                )
            };
            // Everything on a cloned child came from the template, which already declares the
            // field's type and parameters. Restating them makes the entry differ from an
            // authored one and, worse, freezes a copy that stops tracking the template. Keep
            // the value and the internal markers; drop the rest, and write value-only fields
            // in the dash form authored entries use.
            fn strip_restated(node: &mut OverseerNode) {
                node.parameters
                    .retain(|k, _| k.starts_with('_') || k == "value");
                if node.parameters.contains_key("value") && node.children.is_empty() {
                    // `_original_type` is what the emitter consults to decide between a type
                    // keyword and dash form; "-" is how an authored `- name = value` override
                    // is represented.
                    node.authored_dash = true;
                    node.parameters.insert(
                        "_original_type".to_string(),
                        OverseerValue::String("-".to_string()),
                    );
                }
                for child in node.children.iter_mut() {
                    strip_restated(child);
                }
            }

            let mut distinguishing = node.clone();
            distinguishing
                .parameters
                .remove("_materialized_from_template");
            distinguishing
                .children
                .retain(|c| keeps_a_value(c) || (c.node_type == "list" && !c.children.is_empty()));
            for child in distinguishing.children.iter_mut() {
                strip_restated(child);
            }
            return Self::serialize_node_context(
                &distinguishing,
                output,
                indent_level,
                in_list_body,
                fallback_indent_unit,
                list_indent_hint,
                force_emit_template_children,
            );
        }

        if node.node_type == "mount" && !node.children.is_empty() {
            let mut declaration_only = node.clone();
            declaration_only.children.clear();
            return Self::serialize_node_context(
                &declaration_only,
                output,
                indent_level,
                in_list_body,
                fallback_indent_unit,
                list_indent_hint,
                force_emit_template_children,
            );
        }

        let snapshot = Self::snapshot_for(node);
        let fallback_indent_unit = if fallback_indent_unit.is_empty() {
            "    "
        } else {
            fallback_indent_unit
        };

        let inline_patched = if let Some(snap) = snapshot.as_ref() {
            if matches!(snap.origin, SnapshotOrigin::Parsed) {
                Self::try_inline_patch_snapshot(node, snap)
            } else {
                None
            }
        } else {
            None
        };

        let leading_whitespace_only = snapshot
            .as_ref()
            .map(|snap| !snap.leading_trivia.is_empty() && snap.leading_trivia.trim().is_empty())
            .unwrap_or(false);
        let allow_snapshot_trivia = if in_list_body {
            leading_whitespace_only
        } else {
            true
        };

        // An instance someone has written into is given back as the instance wrote it, with what
        // changed since spliced in: the merged node holds the whole template around what it wrote,
        // and rebuilding the block from that lost every unnamed step, `+` and nested override in it.
        if Self::is_a_written_instance(node) {
            if let Some(snap) = snapshot.as_ref() {
                let unchanged = node.source_fingerprint == Some(snap.fingerprint)
                    && Self::children_match_snapshots(node);
                let indent = fallback_indent_unit.repeat(indent_level);
                let rewritten = if unchanged {
                    Some(None)
                } else {
                    Self::rewrite_written(node, &indent, fallback_indent_unit)?
                };
                if let Some(rewritten) = rewritten {
                    let emitted = !snap.leading_trivia.is_empty()
                        && Self::emit_leading_trivia(
                            output,
                            &snap.leading_trivia,
                            node,
                            allow_snapshot_trivia,
                            indent_level,
                            in_list_body,
                        );
                    if !emitted {
                        // Its own line, at the indent it was read at, as the trivia would
                        // have put it.
                        let line_start = output.rfind('\n').map(|at| at + 1).unwrap_or(0);
                        if !output[line_start..].trim().is_empty() {
                            while output.ends_with(' ') || output.ends_with('\t') {
                                output.pop();
                            }
                            output.push('\n');
                        } else {
                            output.truncate(line_start);
                        }
                        let own = snap
                            .leading_trivia
                            .rsplit('\n')
                            .next()
                            .filter(|last| snap.leading_trivia.contains('\n') && last.trim().is_empty())
                            .map(|last| last.trim_end_matches('\r').to_string());
                        output.push_str(
                            &own.unwrap_or_else(|| fallback_indent_unit.repeat(indent_level)),
                        );
                    }
                    let text = rewritten
                        .unwrap_or_else(|| snap.full_text.replace("\r\n", "\n"));
                    output.push_str(&text);
                    return Ok(());
                }
            }
        }

        if force_emit_template_children {
            if let Some(snap) = snapshot.as_ref() {
                if matches!(snap.origin, SnapshotOrigin::Parsed)
                    && Self::children_match_snapshots(node)
                {
                    if !snap.leading_trivia.is_empty() {
                        Self::emit_leading_trivia(
                            output,
                            &snap.leading_trivia,
                            node,
                            allow_snapshot_trivia,
                            indent_level,
                            in_list_body,
                        );
                    }
                    if snap.full_text.contains('\r') {
                        let normalized = snap.full_text.replace("\r\n", "\n");
                        output.push_str(&normalized);
                    } else {
                        output.push_str(&snap.full_text);
                    }
                    return Ok(());
                }
            }
        }

        if let (Some(snap), Some(patched_text)) = (snapshot.as_ref(), inline_patched.as_ref()) {
            if !snap.leading_trivia.is_empty() {
                Self::emit_leading_trivia(
                    output,
                    &snap.leading_trivia,
                    node,
                    allow_snapshot_trivia,
                    indent_level,
                    in_list_body,
                );
            }
            if patched_text.contains('\r') {
                let normalized = patched_text.replace("\r\n", "\n");
                output.push_str(&normalized);
            } else {
                output.push_str(patched_text);
            }
            return Ok(());
        }

        if let (Some(snap), Some(fingerprint)) = (snapshot.as_ref(), node.source_fingerprint) {
            if matches!(snap.origin, SnapshotOrigin::Parsed) && fingerprint == snap.fingerprint {
                if Self::children_match_snapshots(node) {
                    Self::emit_leading_trivia(
                        output,
                        &snap.leading_trivia,
                        node,
                        allow_snapshot_trivia,
                        indent_level,
                        in_list_body,
                    );
                    if snap.full_text.contains('\r') {
                        let normalized = snap.full_text.replace("\r\n", "\n");
                        output.push_str(&normalized);
                    } else {
                        output.push_str(&snap.full_text);
                    }
                    return Ok(());
                }
            }
        }

        let mut emitted_leading_trivia = false;
        if let Some(snap) = snapshot.as_ref() {
            if Self::emit_leading_trivia(
                output,
                &snap.leading_trivia,
                node,
                allow_snapshot_trivia,
                indent_level,
                in_list_body,
            )
            {
                emitted_leading_trivia = true;
            }
        }

        if !emitted_leading_trivia {
            let mut blank_lines_to_emit = node.leading_blank_lines;
            if in_list_body {
                blank_lines_to_emit = 0;
            }
            if !output.is_empty() {
                if indent_level == 0 {
                    blank_lines_to_emit = 0;
                } else {
                    blank_lines_to_emit = blank_lines_to_emit.saturating_sub(1);
                }
            }
            for _ in 0..blank_lines_to_emit {
                output.push('\n');
            }
            if !output.is_empty() && !output.ends_with('\n') {
                    while output.ends_with(' ') || output.ends_with('\t') {
                        output.pop();
                    }
                output.push('\n');
            }
        }

        let snapshot_indent = snapshot
            .as_ref()
            .and_then(|snap| snap.indent_unit.clone())
            .filter(|ind| !ind.is_empty());
        let indent = snapshot_indent
            .clone()
            .unwrap_or_else(|| fallback_indent_unit.repeat(indent_level));
        let needs_indent = !emitted_leading_trivia || output.ends_with('\n') || output.is_empty();
        if needs_indent {
            output.push_str(&indent);
        }

        // Handle list-style items which start with '-'
    if in_list_body
            || node.node_type == "list_item"
            || (indent_level > 0 && node.node_type == "-" && !node.authored_plus)
        {
            // Special handling for real list bodies vs non-list contexts
            // Where the opening of this entry actually lands, which is not always the
            // indent computed above: inside a list body it is lined up with the entries
            // around it. The closing brace has to follow, or an entry opens at one depth
            // and closes at another - which is what a newly added entry did, and why the
            // next save reflowed it and differed from the one before.
            let mut entry_indent: Option<String> = None;
            if in_list_body {
                let mut required_indent = snapshot.as_ref().and_then(|snap| {
                    let text = if snap.full_text.contains("\r\n") {
                        snap.full_text.replace("\r\n", "\n")
                    } else {
                        snap.full_text.to_string()
                    };
                    for line in text.lines() {
                        if line.trim().is_empty() {
                            continue;
                        }
                        if let Some(dash_idx) = line.find('-') {
                            let prefix = &line[..dash_idx];
                            if prefix.chars().all(|c| c == ' ') {
                                return Some(prefix.len());
                            }
                        }
                        break;
                    }
                    None
                });
                if let Some(hint) = list_indent_hint {
                    required_indent = Some(required_indent.map(|val| val.max(hint)).unwrap_or(hint));
                }
                // Nothing to copy from: no neighbour to line up with, and no text of its own to
                // read one out of. That is the first entry appended to a list, and the honest
                // answer for it is how deep the node actually is - which `indent` already says.
                //
                // Saying it here rather than leaving it unset matters because the entry's line
                // has usually been opened already, at whatever depth the enclosing block left
                // the cursor at. Unset, the correction below had nothing to correct to and the
                // entry stayed where the cursor happened to be: for a list nested inside a
                // template entry, its parent list's depth, two levels short.
                let required_indent = required_indent.or(Some(indent.len()));
                if let Some(required_indent) = required_indent {
                    entry_indent = Some(" ".repeat(required_indent));
                    let line_start = output.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
                    let current_slice = &output[line_start..];
                    let current_spaces = current_slice.chars().take_while(|c| *c == ' ').count();
                    if current_spaces < required_indent {
                        output.push_str(&" ".repeat(required_indent - current_spaces));
                    } else if current_spaces > required_indent {
                        let mut to_remove = current_spaces - required_indent;
                        while to_remove > 0 && output.ends_with(' ') {
                            output.pop();
                            to_remove -= 1;
                        }
                    }
                }
                // Determine if this is a simple primitive list item (only when not template-derived)
                let is_template_instance = node.template.is_some()
                    || matches!(
                        node.parameters.get("_from_template"),
                        Some(OverseerValue::Boolean(true))
                    );
                if let Some(value) = node.parameters.get("value") {
                    // Only emit as simple value when NOT a template instance (true primitive lists)
                    if !is_template_instance {
                        output.push_str("- ");
                        output.push_str(&Self::serialize_value_with_node(node, value));
                        output.push('\n');
                        Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
                        return Ok(());
                    }
                }
                // Complex/template-based item: emit as "- name { ... }" (name optional) and use the standard child emission (with concise override rules)
                output.push_str("- ");
                let entry_name = node.name.trim();
                let has_displayable_name = !entry_name.is_empty()
                    && entry_name != "-"
                    && !Self::is_auto_generated_entry_name(entry_name);
                if has_displayable_name {
                    output.push_str(entry_name);
                    output.push_str(" {\n");
                } else {
                    output.push_str("{\n");
                }
                let body_start = output.len();
                // Suppress template-derived children for instances and emit concise overrides
                let suppress_template_children = node.template.is_some()
                    || matches!(
                        node.parameters.get("_from_template"),
                        Some(OverseerValue::Boolean(true))
                    )
                    || matches!(
                        node.parameters.get("_template_node"),
                        Some(OverseerValue::Boolean(true))
                    )
                    || node
                        .parameters
                        .keys()
                        .any(|k| Self::marks_a_template_node(k));
                // Pre-parse explicit override names list on the instance (if present)
                // NOTE: Do not use this list to filter which overrides to persist. Users can introduce
                // new overrides at runtime (e.g., by editing a field), and this list may be stale.
                // We keep parsing it for potential future use, but we won't gate emission on it.
                let _explicit_names_ignored: Option<Vec<String>> =
                    if let Some(OverseerValue::String(list)) =
                        node.parameters.get("_explicit_overrides")
                    {
                        let v: Vec<String> = list
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect();
                        Some(v)
                    } else {
                        None
                    };
                for child in &node.children {
                    let child_snapshot = Self::snapshot_for(child);
                    let child_indent = child_snapshot
                        .as_ref()
                        .and_then(|snap| snap.indent_unit.clone())
                        .unwrap_or_else(|| fallback_indent_unit.repeat(indent_level + 1));
                    let is_template_child_flag = matches!(
                        child.parameters.get("_template_node"),
                        Some(OverseerValue::Boolean(true))
                    );
                    let has_template_param_markers =
                        child.parameters.keys().any(|k| Self::marks_a_template_node(k));
                    let is_template_child = is_template_child_flag || has_template_param_markers;
                    let has_explicit_override = matches!(
                        child.parameters.get("_explicit_child_override"),
                        Some(OverseerValue::Boolean(true))
                    );
                    // Previously we filtered explicit overrides against an instance-level list (_explicit_overrides).
                    // This caused edits added later to be dropped. Do not gate on that list anymore.

                    // Special-case: if this child is a transparent wrapper and any of its descendants
                    // were explicitly overridden, emit those descendant overrides concisely here and skip the wrapper.
                    //
                    // Values only, though: a list in the wrapper that the entry wrote into is no
                    // `- name = value`, and hoisting the values beside it would skip it. Such a
                    // wrapper is left to the branch below, which writes both.
                    if suppress_template_children
                        && is_template_child
                        && child.is_hierarchy_transparent
                        && !Self::wrapper_holds_a_written_list(child)
                    {
                        // Collect descendant value-only overrides that should be emitted concisely.
                        // Criteria:
                        //  - value present AND no non-internal, non-value params AND no children
                        //  - AND (explicit override marker present OR value differs from template default if available)
                        fn collect_descendant_value_overrides<'a>(
                            node: &'a OverseerNode,
                            _name_filter_unused: &Option<Vec<String>>,
                            out: &mut Vec<(&'a str, &'a OverseerValue)>,
                        ) {
                            // Check current node
                            let has_value = node.parameters.contains_key("value");
                            if has_value {
                                let non_internal_non_value_params = node
                                    .parameters
                                    .iter()
                                    .filter(|(k, _)| {
                                        let ks = k.as_str();
                                        if ks.starts_with('_') || ks == "value" {
                                            return false;
                                        }
                                        // Ignore template-derived params (have a corresponding _template_ marker)
                                        let marker = format!("_template_{}", ks);
                                        !node.parameters.contains_key(&marker)
                                    })
                                    .count();
                                let only_value_override =
                                    non_internal_non_value_params == 0 && node.children.is_empty();
                                if only_value_override {
                                    let explicit = matches!(
                                        node.parameters.get("_explicit_child_override"),
                                        Some(OverseerValue::Boolean(true))
                                    );
                                    let val = node.parameters.get("value").unwrap();
                                    let differs_from_template =
                                        match node.parameters.get("_template_value") {
                                            Some(tv) => tv != val,
                                            None => false,
                                        };
                                    if explicit || differs_from_template {
                                        out.push((node.name.as_str(), val));
                                    }
                                }
                            }
                            // Down through wrappers and into leaves, and no further. What is
                            // hoisted here is written as `- name = value` on the entry, which
                            // is only true of something that is effectively the entry's own
                            // child - so a node standing for itself ends the search, whatever
                            // is inside it.
                            //
                            // Without that, a template instance used inside a wrapper had its
                            // own overrides copied into every record: `- input = $(../../amount)`
                            // on each of them, saying something the entry never said and could
                            // not have meant.
                            for ch in &node.children {
                                let stands_for_itself =
                                    !ch.children.is_empty() && !ch.is_hierarchy_transparent;
                                if stands_for_itself {
                                    continue;
                                }
                                collect_descendant_value_overrides(ch, _name_filter_unused, out);
                            }
                        }
                        let mut desc_overrides: Vec<(&str, &OverseerValue)> = Vec::new();
                        // Always collect any explicit descendant value overrides; do not filter by instance list.
                        collect_descendant_value_overrides(
                            child,
                            &_explicit_names_ignored,
                            &mut desc_overrides,
                        );
                        if !desc_overrides.is_empty() {
                            Self::drop_trailing_whitespace_line(output);
                            if !output.ends_with('\n') {
                                output.push('\n');
                            }
                            for (n, v) in desc_overrides {
                                output.push_str(&format!(
                                    "{}- {} = {}\n",
                                    child_indent,
                                    n,
                                    Self::serialize_value_with_node(child, v)
                                ));
                            }
                            continue; // Skip normal emission of the transparent wrapper
                        }
                    }
                    // Skip template-derived children that are not explicitly overridden - but
                    // not before asking whether anything was written *inside* one.
                    //
                    // A container standing for itself - a named div, a nested record - carries no
                    // override of its own when a field within it is set, because the marker goes
                    // on the field. So the container was skipped and everything written into it
                    // went with it: the write landed in memory, the action reported the address it
                    // had written, and the document was serialized without it. That is a write
                    // reporting success and changing nothing, and it is why setting a day's target
                    // in tracker_v2 - two containers below the entry - never reached the file,
                    // while a field the entry holds directly always did.
                    //
                    // An entry cannot say `- target_calories = 2000` for this: that names a child
                    // of the entry, and this one is a child of `targets`. It says it the way a
                    // nested list is already said - the container named, the overrides inside.
                    if suppress_template_children && is_template_child && (!has_explicit_override) {
                        if !child.children.is_empty() && child.node_type == "div" {
                            // A wrapper stands for nothing and is not named, so what is inside it
                            // is written as the entry's own - which is what its address says too.
                            let names_itself = !crate::addressing::is_wrapper(child);
                            let (body_indent, body_level) = if names_itself {
                                (format!("{}{}", child_indent, fallback_indent_unit), indent_level + 2)
                            } else {
                                (child_indent.clone(), indent_level + 1)
                            };
                            let mut inside = String::new();
                            if Self::write_overrides_inside(
                                child,
                                &mut inside,
                                &body_indent,
                                body_level,
                                fallback_indent_unit,
                            )? {
                                Self::drop_trailing_whitespace_line(output);
                                if !output.ends_with('\n') {
                                    output.push('\n');
                                }
                                if names_itself {
                                    output.push_str(&format!(
                                        "{}{} {} {{\n",
                                        child_indent, child.node_type, child.name
                                    ));
                                }
                                output.push_str(&inside);
                                if names_itself {
                                    output.push_str(&format!("{}}}\n", child_indent));
                                }
                            }
                        }
                        continue;
                    }
                    if suppress_template_children {
                        let has_value = child.parameters.contains_key("value");
                        let non_internal_non_value_params = child
                            .parameters
                            .iter()
                            .filter(|(k, _)| {
                                let ks = k.as_str();
                                if ks.starts_with('_') || ks == "value" {
                                    return false;
                                }
                                let marker = format!("_template_{}", ks);
                                !child.parameters.contains_key(&marker)
                            })
                            .count();
                        let only_value_override = has_value
                            && non_internal_non_value_params == 0
                            && Self::has_only_the_templates_body(child);
                        if only_value_override {
                            let val = child.parameters.get("value").unwrap();
                            let differs_from_template =
                                match child.parameters.get("_template_value") {
                                    Some(tv) => tv != val,
                                    None => false,
                                };
                            if has_explicit_override || differs_from_template {
                                Self::drop_trailing_whitespace_line(output);
                                if !output.ends_with('\n') {
                                    output.push('\n');
                                }
                                output.push_str(&format!(
                                    "{}- {} = {}\n",
                                    child_indent,
                                    child.name,
                                    Self::serialize_value_with_node(child, val)
                                ));
                                continue;
                            }
                        }
                    }
                    // Fallback: serialize child normally inside the block (not as list body)
                    // so field lines and nested blocks render correctly.
                    Self::serialize_node_context(
                        child,
                        output,
                        indent_level + 1,
                        false,
                        fallback_indent_unit,
                        None,
                        false,
                    )?;
                }
                if node.children.is_empty() {
                    if let Some(body_text) = snapshot
                        .as_ref()
                        .and_then(|snap| Self::snapshot_block_inner(snap))
                    {
                        Self::push_trivia(output, &body_text);
                    }
                }
                // An entry that says nothing of its own - every field the template's - is `- { }`,
                // as an empty list is, rather than a brace on a line of its own.
                if output[body_start..].trim().is_empty() {
                    output.truncate(body_start - 1);
                    output.push_str(" }\n");
                    Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
                    return Ok(());
                }
                while output.ends_with(' ') || output.ends_with('\t') {
                    output.pop();
                }
                if !output.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str(entry_indent.as_deref().unwrap_or(&indent));
                output.push_str("}\n");
                Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
                return Ok(());
            } else {
                // Non-list context override: allow "- name = value" syntax
                if let Some(value) = node.parameters.get("value") {
                    if !node.name.is_empty() && node.children.is_empty() {
                        output.push_str("- ");
                        output.push_str(&node.name);
                        output.push_str(" = ");
                        output.push_str(&Self::serialize_value_with_node(node, value));
                        output.push('\n');
                        Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
                        return Ok(());
                    }
                }
                // Non-list context override: named nested block "- name { ... }"
                if !node.name.is_empty() && !node.children.is_empty() {
                    output.push_str("- ");
                    output.push_str(&node.name);
                    output.push_str(" {\n");
                    for child in &node.children {
                        Self::serialize_node_context(
                            child,
                            output,
                            indent_level + 1,
                            false,
                            fallback_indent_unit,
                            None,
                            false,
                        )?;
                    }
                    output.push_str(&format!("{}}}\n", indent));
                    Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
                    return Ok(());
                }
                // Else: fall through to normal serialization
            }
        } else {
            // For children that belong to a list entry object (parent emitted as "- {")
            // we want each field line to start as a list-style override "- name = value" when possible.
            if in_list_body {
                // In list bodies, if this is a simple value field or a parameter-only node with value,
                // write as an inline override: "- name = value" and return.
                if let Some(val) = node.parameters.get("value") {
                    if !node.name.is_empty() && node.children.is_empty() {
                        output.push_str("- ");
                        output.push_str(&node.name);
                        output.push_str(" = ");
                        output.push_str(&Self::serialize_value(val));
                        output.push('\n');
                        Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);
                        return Ok(());
                    }
                }
                // Otherwise, for structural children, use '-' as the node type marker
                output.push('-');
            } else {
                // Handle node type or template path
                if let Some(template_path) = &node.template {
                    output.push_str(&format!("<{}>", template_path));
                } else if let Some(OverseerValue::Template(tpl)) =
                    node.parameters.get("_template_origin")
                {
                    output.push_str(&format!("<{}>", tpl));
                } else if node.authored_plus {
                    // `+` stands where the type does, and alone when the type is inferred.
                    output.push('+');
                    let inferred = node.node_type == "-"
                        || matches!(
                            node.parameters.get("_original_type"),
                            Some(OverseerValue::String(t)) if t == "-"
                        );
                    if !inferred {
                        output.push_str(&node.node_type);
                    }
                } else {
                    // Check if this node had its type resolved and restore original
                    if let Some(OverseerValue::String(original_type)) =
                        node.parameters.get("_original_type")
                    {
                        if original_type == "-" {
                            output.push('-');
                        } else {
                            output.push_str(original_type);
                        }
                    } else {
                        // Use "-" for type-inferred nodes, otherwise use the actual type
                        if node.node_type == "-"
                            || (node.name == "-" && node.node_type != "list_item")
                        {
                            output.push('-');
                        } else {
                            output.push_str(&node.node_type);
                        }
                    }
                }
            }

            // Handle node name - skip if it's "-" (unnamed node marker) or if it's auto-defaulted (name == type)
            if !node.name.is_empty() && node.name != "-" {
                // If the name is exactly the same as the type, treat it as auto-defaulted and omit it.
                let auto_defaulted = node.name == node.node_type;
                if !auto_defaulted {
                    output.push(' ');
                    output.push_str(&node.name);
                }
            }
        }

        // Handle parameters (excluding the special 'value' parameter for fields)
        //
        // Parameters that still say what the header said keep its text - line breaks, spacing
        // and all - since a change to the node's entries is no reason to reformat its header.
        let kept_params = snapshot
            .as_ref()
            .and_then(|snap| snap.header.parameters.as_ref())
            .filter(|slice| Self::params_said(&slice.text) == Self::params_now(node))
            .map(|slice| slice.text.replace("\r\n", "\n"));
        if let Some(params) = kept_params.or_else(|| Self::render_params(node)) {
            output.push(' ');
            output.push_str(&params);
        }

        // Handle body (value assignment, block, both, or nothing)
        //
        // A node may carry a value *and* a body: `int done (...) = 0 { on change { ... } }`.
        // This used to take the value branch and drop the children on the floor, silently -
        // the serializer half of why a field could not have a handler. The value is written
        // here and, when there are children, the block arm below writes them.
        let assigned = node
            .parameters
            .get("value")
            .map(|value| Self::serialize_value_with_node(node, value));
        if let Some(text) = &assigned {
            output.push_str(&format!(" = {}", text));
        }
        if assigned.is_some() && node.children.is_empty() {
            output.push('\n');
        } else if node.children.is_empty() {
            // Replay the authored block, braces and all. A node that had no block at all
            // still gets none - the absence of an envelope in the snapshot is what says so,
            // and inventing braces would be just as much of a change as dropping them.
            //
            // Only a block that held nothing to begin with, though - whitespace, comments. One
            // that held entries and has lost them all is not the block that was authored, and
            // replaying it put back exactly what had been taken out: removing the last entry of
            // a list answered Ok and left the file as it was.
            let authored = snapshot
                .as_ref()
                .and_then(|snap| Self::snapshot_block_verbatim(snap));
            if authored.as_deref().map_or(false, |block| !Self::holds_only_trivia(block)) {
                output.push_str(" {\n");
                output.push_str(&indent);
                output.push('}');
            } else if let Some(block) = authored {
                let spacer = snapshot
                    .as_ref()
                    .map(|snap| snap.header.trailing.as_str())
                    .filter(|t| !t.is_empty())
                    .unwrap_or(" ")
                    .replace("\r\n", "\n");
                output.push_str(&spacer);
                output.push_str(&block);
            } else if let Some(body_text) = snapshot
                .as_ref()
                .and_then(|snap| Self::snapshot_block_inner(snap))
            {
                Self::push_trivia(output, &body_text);
            }
            output.push('\n');
        } else {
            let mut header_trailing = if let Some(snap) = snapshot.as_ref() {
                let trailing = if !snap.header.trailing.is_empty() {
                    if snap.header.trailing.contains("\r\n") {
                        snap.header.trailing.replace("\r\n", "\n")
                    } else {
                        snap.header.trailing.clone()
                    }
                } else {
                    let header_start = snap.header_span.0.saturating_sub(snap.span.0);
                    let header_end = snap.header_span.1.saturating_sub(snap.span.0);
                    if header_end <= snap.full_text.len() && header_start < header_end {
                        let header_text = &snap.full_text[header_start..header_end];
                        let mut whitespace_start = header_text.len();
                        for (idx, ch) in header_text.char_indices().rev() {
                            if ch.is_whitespace() {
                                whitespace_start = idx;
                            } else {
                                break;
                            }
                        }
                        if whitespace_start < header_text.len() {
                            let whitespace = &header_text[whitespace_start..];
                            if whitespace.contains("\r\n") {
                                whitespace.replace("\r\n", "\n")
                            } else {
                                whitespace.to_string()
                            }
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                };
                if trailing.is_empty() {
                    " ".to_string()
                } else {
                    trailing
                }
            } else {
                " ".to_string()
            };
            if !node.children.is_empty() {
                let trimmed = header_trailing.trim_start();
                if trimmed.starts_with('}') {
                    header_trailing = " ".to_string();
                }
            }
            // With a value already written, the snapshot's header trailing is the whitespace
            // that sat before the `=` - replaying it here would put it after the value instead.
            if assigned.is_some() {
                header_trailing = " ".to_string();
            }
            output.push_str(&header_trailing);

            let open_fragment = snapshot
                .as_ref()
                .and_then(|snap| snap.body.child_envelope.open.as_ref())
                .map(|slice| {
                    if slice.text.contains("\r\n") {
                        slice.text.replace("\r\n", "\n")
                    } else {
                        slice.text.clone()
                    }
                })
                .unwrap_or_else(|| "{".to_string());
            let open_fragment_text = open_fragment;
            let open_fragment_start = output.len();
            output.push_str(&open_fragment_text);
            let mut open_fragment_adjusted = false;
            let mut ensure_open_fragment_multiline = |output: &mut String| {
                if !open_fragment_adjusted {
                    output.truncate(open_fragment_start);
                    let mut adjusted = open_fragment_text.replace("\r\n", "\n");
                    if let Some(pos) = adjusted.rfind('{') {
                        adjusted.truncate(pos + 1);
                    } else {
                        adjusted.clear();
                        adjusted.push('{');
                    }
                    adjusted.push('\n');
                    output.push_str(&adjusted);
                    open_fragment_adjusted = true;
                } else if !output.ends_with('\n') {
                    output.push('\n');
                }
            };
            let mut inline_consumed_children: usize = 0;
            let mut open_fragment_needs_multiline = false;
            let trim_trailing_whitespace = |buffer: &mut String| {
                while buffer.ends_with(' ') || buffer.ends_with('\t') {
                    buffer.pop();
                }
                if buffer.ends_with('\n') {
                    buffer.pop();
                    while buffer.ends_with(' ') || buffer.ends_with('\t') {
                        buffer.pop();
                    }
                    buffer.push('\n');
                }
                loop {
                    if !buffer.ends_with('\n') {
                        break;
                    }
                    let mut idx = buffer.len();
                    if idx == 0 {
                        break;
                    }
                    idx -= 1; // point to newline character
                    if idx == 0 {
                        break;
                    }
                    let mut scan = idx;
                    let mut only_ws = true;
                    while scan > 0 {
                        scan -= 1;
                        let ch = buffer.as_bytes()[scan] as char;
                        if ch == '\n' {
                            break;
                        }
                        if !ch.is_whitespace() {
                            only_ws = false;
                            break;
                        }
                    }
                    if only_ws {
                        buffer.truncate(scan + 1);
                    } else {
                        break;
                    }
                }
            };
            if let Some(snap) = snapshot.as_ref() {
                if let Some(open_slice) = snap.body.child_envelope.open.as_ref() {
                    let open_end = open_slice.span.1.saturating_sub(snap.span.0);
                    if open_end < snap.full_text.len() {
                        let remainder = &snap.full_text[open_end..];
                        if let Some(newline_idx) = remainder.find('\n') {
                            if newline_idx > 0 {
                                let inline_segment = &remainder[..newline_idx];
                                if inline_segment.chars().any(|c| !c.is_whitespace()) {
                                    let inline_start_abs = snap.span.0 + open_end;
                                    let inline_end_abs = (inline_start_abs + newline_idx + 1)
                                        .min(snap.span.0 + snap.full_text.len());

                                    let mut inline_child_records: Vec<(usize, std::sync::Arc<NodeSourceSnapshot>)> = Vec::new();
                                    for (idx, child) in node.children.iter().enumerate() {
                                        if let Some(child_snap) = Self::snapshot_for(child) {
                                            let start = child_snap.span.0;
                                            if start >= inline_end_abs {
                                                break;
                                            }
                                            let end = child_snap.span.1;
                                            if start >= inline_start_abs && end <= inline_end_abs {
                                                inline_child_records.push((idx, child_snap));
                                            } else {
                                                break;
                                            }
                                        } else {
                                            inline_child_records.clear();
                                            break;
                                        }
                                    }

                                    if inline_child_records.is_empty() {
                                        let trimmed_inline = inline_segment.trim();
                                        let has_structural_children = !node.children.is_empty();
                                        let should_skip_inline = has_structural_children
                                            && trimmed_inline.starts_with('}')
                                            && trimmed_inline
                                                .chars()
                                                .all(|c| c == '}' || c.is_whitespace());
                                        if has_structural_children {
                                            open_fragment_needs_multiline = true;
                                        } else if !should_skip_inline {
                                            let normalized = inline_segment.replace("\r\n", "\n");
                                            output.push_str(&normalized);
                                            if !output.ends_with('\n') {
                                                output.push('\n');
                                            }
                                        } else {
                                            open_fragment_needs_multiline = true;
                                        }
                                    } else {
                                        let any_changed = inline_child_records.iter().any(|(idx, child_snap)| {
                                            match node.children[*idx].source_fingerprint {
                                                Some(fp) => fp != child_snap.fingerprint,
                                                None => true,
                                            }
                                        });

                                        let mut consumed_inline_children = false;
                                        if any_changed {
                                            if let Some(patched) = Self::reconstruct_inline_segment(
                                                node,
                                                snap,
                                                inline_start_abs,
                                                inline_end_abs,
                                                &inline_child_records,
                                            ) {
                                                output.push_str(&patched);
                                                consumed_inline_children = true;
                                            } else {
                                                open_fragment_needs_multiline = true;
                                            }
                                        } else {
                                            let normalized = inline_segment.replace("\r\n", "\n");
                                            output.push_str(&normalized);
                                            consumed_inline_children = true;
                                        }

                                        if consumed_inline_children {
                                            if !output.ends_with('\n') {
                                                output.push('\n');
                                            }
                                            inline_consumed_children = inline_child_records.len();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Children under a list node are list-body items (render as '-')
            let children_in_list_body = node.node_type == "list";
            let mut list_indent_hints: VecDeque<usize> = VecDeque::new();
            let mut last_list_indent_hint: Option<usize> = None;
            if children_in_list_body {
                if let Some(snap) = snapshot.as_ref() {
                    if let Some(body_text) = Self::snapshot_block_inner(snap) {
                        let mut depth: i32 = 0;
                        for line in body_text.lines() {
                            if line.trim().is_empty() {
                                continue;
                            }
                            let leading_spaces = line.chars().take_while(|c| *c == ' ').count();
                            let content = &line[leading_spaces..];
                            if depth == 0 && content.starts_with("- {") {
                                list_indent_hints.push_back(leading_spaces);
                            }
                            let open_count = content.chars().filter(|c| *c == '{').count() as i32;
                            let close_count = content.chars().filter(|c| *c == '}').count() as i32;
                            depth += open_count;
                            depth -= close_count;
                            if depth < 0 {
                                depth = 0;
                            }
                        }
                    }
                }
            }
            // Suppress template-derived children for any node that originated from a template (standalone instances or list entries)
            let suppress_template_children = node.template.is_some()
                || matches!(
                    node.parameters.get("_from_template"),
                    Some(OverseerValue::Boolean(true))
                )
                || matches!(
                    node.parameters.get("_template_node"),
                    Some(OverseerValue::Boolean(true))
                )
                || node
                    .parameters
                    .keys()
                    .any(|k| Self::marks_a_template_node(k));
            // Pre-parse explicit override names list on the instance (if present)
            let explicit_names: Option<Vec<String>> = if let Some(OverseerValue::String(list)) =
                node.parameters.get("_explicit_overrides")
            {
                let v: Vec<String> = list
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                Some(v)
            } else {
                None
            };
            let mut inline_consumed = inline_consumed_children;
            for child in &node.children {
                if inline_consumed > 0 {
                    inline_consumed -= 1;
                    continue;
                }
                if open_fragment_needs_multiline {
                    ensure_open_fragment_multiline(output);
                    open_fragment_needs_multiline = false;
                    trim_trailing_whitespace(output);
                }
                let child_snapshot = Self::snapshot_for(child);
                let child_indent = child_snapshot
                    .as_ref()
                    .and_then(|snap| snap.indent_unit.clone())
                    .unwrap_or_else(|| fallback_indent_unit.repeat(indent_level + 1));
                // Skip template-derived children unless they were explicitly overridden
                let is_template_child_flag = matches!(
                    child.parameters.get("_template_node"),
                    Some(OverseerValue::Boolean(true))
                );
                let has_template_param_markers =
                    child.parameters.keys().any(|k| Self::marks_a_template_node(k));
                let is_template_child = is_template_child_flag || has_template_param_markers;
                // Only treat as includeable if it was explicitly overridden by the source, not just equal/diff logic
                let has_explicit_override = matches!(
                    child.parameters.get("_explicit_child_override"),
                    Some(OverseerValue::Boolean(true))
                );
                // Additional safety: in a template instance, only consider overrides that were explicitly named in the instance source
                let mut listed_in_instance_overrides = true;
                if suppress_template_children {
                    if let Some(names) = &explicit_names {
                        if !names.is_empty() {
                            listed_in_instance_overrides = names.iter().any(|n| n == &child.name);
                        }
                    }
                }
                let is_template_list_entry = node.node_type == "list"
                    && matches!(
                        node.parameters.get("entry"),
                        Some(OverseerValue::Template(entry_type))
                            if crate::bases::reference(entry_type).name == child.node_type
                    );

                // A wrapper holding a list written into is written through, list and values both:
                // hoisting the values below would also take the list's own records for values of
                // this node, and leave the list out.
                if suppress_template_children
                    && is_template_child
                    && crate::addressing::is_wrapper(child)
                    && Self::wrapper_holds_a_written_list(child)
                {
                    let mut inside = String::new();
                    if Self::write_overrides_inside(
                        child,
                        &mut inside,
                        &child_indent,
                        indent_level + 1,
                        fallback_indent_unit,
                    )? {
                        Self::drop_trailing_whitespace_line(output);
                        trim_trailing_whitespace(output);
                        if !output.ends_with('\n') {
                            output.push('\n');
                        }
                        output.push_str(&inside);
                    }
                    continue;
                }
                // Special-case: if child is a transparent wrapper and any of its descendants were explicitly overridden,
                // emit those descendant overrides concisely here and skip the wrapper itself.
                if suppress_template_children && is_template_child && child.is_hierarchy_transparent
                {
                    fn collect_descendant_value_overrides<'a>(
                        node: &'a OverseerNode,
                        name_filter: &Option<Vec<String>>,
                        out: &mut Vec<(&'a str, &'a OverseerValue)>,
                    ) {
                        let has_explicit = matches!(
                            node.parameters.get("_explicit_child_override"),
                            Some(OverseerValue::Boolean(true))
                        );
                        let name_matches = match name_filter {
                            Some(v) if !v.is_empty() => v.iter().any(|n| n == &node.name),
                            _ => true,
                        };
                        if has_explicit && name_matches {
                            let has_value = node.parameters.contains_key("value");
                            let non_internal_non_value_params = node
                                .parameters
                                .iter()
                                .filter(|(k, _)| {
                                    let ks = k.as_str();
                                    !ks.starts_with('_') && ks != "value"
                                })
                                .count();
                            let only_value_override = has_value
                                && non_internal_non_value_params == 0
                                && node.children.is_empty();
                            let has_template_value_marker =
                                node.parameters.contains_key("_template_value");
                            if only_value_override && !has_template_value_marker {
                                out.push((
                                    node.name.as_str(),
                                    node.parameters.get("value").unwrap(),
                                ));
                            }
                        }
                        for ch in &node.children {
                            collect_descendant_value_overrides(ch, name_filter, out);
                        }
                    }
                    let mut desc_overrides: Vec<(&str, &OverseerValue)> = Vec::new();
                    collect_descendant_value_overrides(child, &explicit_names, &mut desc_overrides);
                    if !desc_overrides.is_empty() {
                        trim_trailing_whitespace(output);
                        if !output.ends_with('\n') {
                            output.push('\n');
                        }
                        for (n, v) in desc_overrides {
                            output.push_str(&format!(
                                "{}- {} = {}\n",
                                child_indent,
                                n,
                                Self::serialize_value_with_node(child, v)
                            ));
                        }
                        continue;
                    }
                }
                if suppress_template_children
                    && is_template_child
                    && !is_template_list_entry
                    && (!has_explicit_override || !listed_in_instance_overrides)
                {
                    continue;
                }
                // For template instances/clones, if a child was overridden with only a simple value, prefer the concise "- name = value" form
                if suppress_template_children
                    && has_explicit_override
                    && listed_in_instance_overrides
                {
                    let has_value = child.parameters.contains_key("value");
                    let non_internal_non_value_params = child
                        .parameters
                        .iter()
                        .filter(|(k, _)| {
                            let ks = k.as_str();
                            // allow 'value' only; ignore internal keys starting with '_'
                            if ks.starts_with('_') || ks == "value" {
                                return false;
                            }
                            // also ignore params that are template-derived (paired _template_param exists)
                            let marker = format!("_template_{}", ks);
                            !child.parameters.contains_key(&marker)
                        })
                        .count();
                    let only_value_override = has_value
                        && non_internal_non_value_params == 0
                        && Self::has_only_the_templates_body(child);
                    // Guard: only treat as an explicit value override if the template value marker was removed.
                    let has_template_value_marker =
                        child.parameters.contains_key("_template_value");
                    if only_value_override && !has_template_value_marker {
                        let val = child.parameters.get("value").unwrap();
                        Self::drop_trailing_whitespace_line(output);
                        trim_trailing_whitespace(output);
                        if !output.ends_with('\n') {
                            output.push('\n');
                        }
                        output.push_str(&format!(
                            "{}- {} = {}\n",
                            child_indent,
                            child.name,
                            Self::serialize_value_with_node(child, val)
                        ));
                        continue;
                    }
                }
                let current_list_hint = if children_in_list_body {
                    if let Some(hint) = list_indent_hints.pop_front() {
                        last_list_indent_hint = Some(hint);
                        Some(hint)
                    } else {
                        last_list_indent_hint
                    }
                } else {
                    None
                };
                Self::serialize_node_context(
                    child,
                    output,
                    indent_level + 1,
                    children_in_list_body,
                    fallback_indent_unit,
                    current_list_hint,
                    is_template_list_entry,
                )?;
                trim_trailing_whitespace(output);
            }

            let close_fragment = snapshot
                .as_ref()
                .and_then(|snap| snap.body.child_envelope.close.as_ref())
                .map(|slice| {
                    if slice.text.contains("\r\n") {
                        slice.text.replace("\r\n", "\n")
                    } else {
                        slice.text.clone()
                    }
                })
                .unwrap_or_else(|| format!("{}}}", indent));
            let close_needs_indent = close_fragment
                .chars()
                .next()
                .map(|c| !c.is_whitespace())
                .unwrap_or(false);
            if output.ends_with('\n') && close_needs_indent {
                output.push_str(&indent);
            }
            output.push_str(&close_fragment);
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }

        Self::emit_trailing_trivia_if_allowed(node, &snapshot, output);

        Ok(())
    }

    /// A node's own parameters as written in its header, `(k=v, ...)`, or nothing when it has none.
    fn render_params(node: &OverseerNode) -> Option<String> {
        let regular_params = Self::collect_serializable_params(node);
        if regular_params.is_empty() {
            return None;
        }
        let params_str: Vec<String> = regular_params
            .iter()
            .map(|(k, v)| {
                let value_str = if k.as_str() == "entry" {
                    match v {
                        OverseerValue::String(s) => s.clone(),
                        OverseerValue::Template(t) => format!("<{}>", t),
                        other => Self::serialize_value_with_node(node, other),
                    }
                } else {
                    Self::serialize_value_with_node(node, v)
                };
                format!("{}={}", k, value_str)
            })
            .collect();
        Some(format!("({})", params_str.join(", ")))
    }

    /// A template instance, or a list entry made from one, whose block can be given back as it
    /// was written: it was parsed from this document and has a block of its own to splice into.
    fn is_a_written_instance(node: &OverseerNode) -> bool {
        matches!(
            node.parameters.get("_from_template"),
            Some(OverseerValue::Boolean(true))
        ) && !matches!(
            node.parameters.get("_materialized_from_template"),
            Some(OverseerValue::Boolean(true))
        ) && Self::snapshot_for(node).is_some_and(|snap| {
            matches!(snap.origin, SnapshotOrigin::Parsed)
                && snap.body.child_envelope.open.is_some()
                && snap.body.child_envelope.close.is_some()
        })
    }

    /// Where in its block a node was written, and how many unnamed nodes below the block's own
    /// node it landed - what the resolver's walk recorded when it merged the block.
    fn written_mark(node: &OverseerNode) -> Option<(usize, usize)> {
        match (
            node.parameters.get("_written_at"),
            node.parameters.get("_written_up"),
        ) {
            (Some(OverseerValue::Integer(at)), Some(OverseerValue::Integer(up))) => {
                Some((*at as usize, *up as usize))
            }
            _ => None,
        }
    }

    fn is_template_derived(node: &OverseerNode) -> bool {
        matches!(
            node.parameters.get("_template_node"),
            Some(OverseerValue::Boolean(true))
        ) || node.parameters.keys().any(|k| Self::marks_a_template_node(k))
    }

    /// Whether the walk goes through a node: an unnamed one, or one the address does not name.
    /// The resolver's `walk_paths` answers the same question.
    fn walked_through(node: &OverseerNode) -> bool {
        node.node_type != "list"
            && (node.is_hierarchy_transparent || node.name.is_empty() || node.name == node.node_type)
    }

    /// What was set on a node the template gave, rather than inherited with it: a value written
    /// as an override, or a parameter that no longer carries its template's marker.
    fn holds_an_edit_of_its_own(node: &OverseerNode) -> bool {
        // Whatever writes to a node clears the fingerprint it was read with. One that still
        // matches holds what it was given, override flags and all: a template's own instances
        // set those too, and a copy of the template resolved after it was copied keeps them.
        let untouched = Self::snapshot_for(node)
            .is_some_and(|snap| node.source_fingerprint == Some(snap.fingerprint));
        if untouched {
            return false;
        }
        Self::value_set_since(node) || !Self::collect_serializable_params(node).is_empty()
    }

    /// Whether a node the template gave holds a value set on it since. The override flag alone
    /// does not say so: a template's own overrides carry it too, into every copy of the template
    /// below the copy's first level. A value set since also no longer is the template's - its
    /// marker taken off by the action that set it, or differing from it when the page set it.
    fn value_set_since(node: &OverseerNode) -> bool {
        if !matches!(
            node.parameters.get("_explicit_child_override"),
            Some(OverseerValue::Boolean(true))
        ) {
            return false;
        }
        let Some(value) = node.parameters.get("value") else {
            return false;
        };
        match node.parameters.get("_template_value") {
            Some(given) => given != value,
            None => true,
        }
    }

    /// The pieces of a block: what it wrote itself, found wherever the walk put it, and what has
    /// been set since on what the template gave, placed after the written element before it.
    ///
    /// `up` counts the unnamed nodes passed on the way down, so a written element is taken only by
    /// the block that wrote it - a `- notes` written on the instance stays the instance's even
    /// when the walk also matched the unnamed `div` it sits in. Edits are taken only by the
    /// nearest written node above them (`take_edits`), so that none is written twice.
    ///
    /// Below a node no block of this document wrote, what the walk marked was written by the
    /// template's own instances, and is the template's: `marks` is then false, and such a node is
    /// read as any other the template gave.
    fn gather_block<'a>(
        children: &'a [OverseerNode],
        up: usize,
        take_edits: bool,
        marks: bool,
        anchor: &mut Option<usize>,
        pieces: &mut Vec<BlockPiece<'a>>,
    ) {
        for child in children {
            let mark = Self::written_mark(child);
            if let (true, Some((at, child_up))) = (marks, mark) {
                if child_up == up {
                    pieces.push(BlockPiece::Written(at, child));
                    *anchor = Some(at);
                }
                // Written by a block nearer to it, or by this one: either way what it holds is
                // that block's, except what this block put through it on its way down.
                if Self::walked_through(child) {
                    Self::gather_block(&child.children, up + 1, false, marks, anchor, pieces);
                }
                continue;
            }
            let given = Self::is_template_derived(child) || mark.is_some();
            if Self::walked_through(child)
                && given
                && !matches!(
                    child.parameters.get("_from_template"),
                    Some(OverseerValue::Boolean(true))
                )
            {
                Self::gather_block(&child.children, up + 1, take_edits, marks, anchor, pieces);
                continue;
            }
            if take_edits && Self::was_set_since(child, given) {
                pieces.push(BlockPiece::Added(*anchor, child));
            }
        }
    }

    /// Whether a node no block wrote holds something that has to be written now: an edit of its
    /// own or one inside it, a list given records, or a node made at runtime.
    fn was_set_since(node: &OverseerNode, given: bool) -> bool {
        if !given || Self::is_a_written_list(node) {
            return true;
        }
        if node.node_type == "list" {
            return false;
        }
        if Self::holds_an_edit_of_its_own(node) {
            return true;
        }
        if Self::is_a_nested_instance(node) {
            return Self::holds_a_marked_edit(node);
        }
        let mut anchor = None;
        let mut inner = Vec::new();
        Self::gather_block(&node.children, 0, true, false, &mut anchor, &mut inner);
        !inner.is_empty()
    }

    /// An instance of another template, inside what a template gave. What is inside it is that
    /// template's and its instance's own overrides, which carry every mark an edit would; walked
    /// into, they were copied into every record that used it - `- input = $(../../amount)` on
    /// each, saying something the entry never said.
    fn is_a_nested_instance(node: &OverseerNode) -> bool {
        matches!(
            node.parameters.get("_from_template"),
            Some(OverseerValue::Boolean(true))
        )
    }

    /// Whether a node, or one inside it, was written to here rather than given by a template.
    fn holds_a_marked_edit(node: &OverseerNode) -> bool {
        matches!(
            node.parameters.get(EDITED_HERE),
            Some(OverseerValue::Boolean(true))
        ) || node.children.iter().any(Self::holds_a_marked_edit)
    }

    /// The pieces inside a nested instance: only what holds an edit marked as made here, found
    /// through the nodes the address does not name.
    fn gather_marked<'a>(children: &'a [OverseerNode], pieces: &mut Vec<BlockPiece<'a>>) {
        for child in children {
            if !Self::holds_a_marked_edit(child) {
                continue;
            }
            if Self::walked_through(child) {
                Self::gather_marked(&child.children, pieces);
            } else {
                pieces.push(BlockPiece::Added(None, child));
            }
        }
    }

    fn line_indent_at(text: &str, at: usize) -> String {
        let line_start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
        text[line_start..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect()
    }

    /// The text of a node a block wrote, with what changed in it since written in, or nothing
    /// when it reads the same. `None` from the outer option means it cannot be said by splicing.
    fn rewrite_written(
        node: &OverseerNode,
        indent: &str,
        unit: &str,
    ) -> Result<Option<Option<String>>> {
        let Some(snap) = Self::snapshot_for(node) else {
            return Ok(None);
        };
        if !matches!(snap.origin, SnapshotOrigin::Parsed) {
            return Ok(None);
        }
        let unchanged_self = node.source_fingerprint == Some(snap.fingerprint);
        // An override whose value has been taken back - a viewer's day, kept out of the file -
        // says nothing any more, and goes.
        if !unchanged_self
            && snap.body.value.is_some()
            && snap.body.child_envelope.open.is_none()
            && !node.parameters.contains_key("value")
        {
            return Ok(Some(Some(String::new())));
        }
        // What is not the template's - a `+` element, one that matched nothing, a list and its
        // records, another instance - is written as any other node is.
        let own_kind = !Self::is_a_written_instance(node)
            && (!Self::is_template_derived(node)
                || node.node_type == "list"
                || matches!(
                    node.parameters.get("_from_template"),
                    Some(OverseerValue::Boolean(true))
                ));
        if own_kind {
            if unchanged_self && Self::children_match_snapshots(node) {
                return Ok(Some(None));
            }
            if node.node_type == "list" {
                if let Some(text) = Self::rewrite_list(node, &snap, indent, unit)? {
                    return Ok(Some(text));
                }
            }
            if node.children.is_empty() {
                if let Some(text) = Self::rebuild_child_with_updated_value(node, &snap) {
                    return Ok(Some(Some(text.replace("\r\n", "\n"))));
                }
            }
            let mut text = String::from("\n");
            let level = indent.len() / unit.len().max(1);
            Self::serialize_node_context(node, &mut text, level, false, unit, None, false)?;
            // The trivia around the node stays where it is in the block's text; only the node's
            // own text is replaced.
            let mut text = text.trim().to_string();
            let leading = snap.leading_trivia.trim();
            if !leading.is_empty() && text.starts_with(leading) {
                text = text[leading.len()..].trim_start().to_string();
            }
            let trailing = snap.trailing_trivia.trim();
            if !trailing.is_empty() && text.ends_with(trailing) {
                text = text[..text.len() - trailing.len()].trim_end().to_string();
            }
            return Ok(Some(Some(text)));
        }

        // Spliced in the text as it was read, whose spans these are; line ends are evened out
        // once the text is whole.
        let full = snap.full_text.to_string();
        let base = snap.span.0;
        let rel = |abs: usize| abs.checked_sub(base).filter(|r| *r <= full.len());
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();

        if !unchanged_self {
            match Self::own_line_changes(node, &snap, full.len()) {
                Some(changes) => replacements.extend(changes),
                None => return Ok(None),
            }
        }

        // The block.
        let mut anchor = None;
        let mut pieces = Vec::new();
        Self::gather_block(&node.children, 0, true, true, &mut anchor, &mut pieces);
        let inner_indent = pieces
            .iter()
            .find_map(|piece| match piece {
                BlockPiece::Written(_, child) => Self::snapshot_for(child)
                    .and_then(|s| rel(s.span.0))
                    .map(|at| Self::line_indent_at(&full, at)),
                BlockPiece::Added(..) => None,
            })
            .unwrap_or_else(|| format!("{}{}", indent, unit));
        let mut ends: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for piece in &pieces {
            let BlockPiece::Written(at, child) = piece else { continue };
            let Some(child_snap) = Self::snapshot_for(child) else {
                return Ok(None);
            };
            let (Some(s), Some(e)) = (rel(child_snap.span.0), rel(child_snap.span.1)) else {
                return Ok(None);
            };
            if s >= e || e > full.len() {
                return Ok(None);
            }
            // A node's text runs on to where the next one starts; the layout after it is the
            // block's, and stays where it is whatever replaces the node.
            let e = s + full[s..e].trim_end().len();
            ends.insert(*at, e);
            match Self::rewrite_written(child, &inner_indent, unit)? {
                None => return Ok(None),
                Some(None) => {}
                Some(Some(text)) => replacements.push(Self::replacing(&full, s, e, &text)),
            }
        }
        let added: Vec<(Option<usize>, &OverseerNode)> = pieces
            .iter()
            .filter_map(|piece| match piece {
                BlockPiece::Added(anchor, child) => Some((*anchor, *child)),
                BlockPiece::Written(..) => None,
            })
            .collect();
        if !added.is_empty() {
            let (Some(open), Some(close)) = (
                snap.body.child_envelope.open.as_ref(),
                snap.body.child_envelope.close.as_ref(),
            ) else {
                return Ok(None);
            };
            let (Some(open_end), Some(close_start)) = (rel(open.span.1), rel(close.span.0)) else {
                return Ok(None);
            };
            let mut by_place: Vec<(usize, String)> = Vec::new();
            for (anchor, child) in &added {
                let mut text = String::new();
                Self::write_added(child, &mut text, &inner_indent, unit, false)?;
                let at = match anchor {
                    Some(a) => match ends.get(a) {
                        Some(e) => *e,
                        None => return Ok(None),
                    },
                    None => open_end,
                };
                by_place.push((at, text));
            }
            let body_is_empty = full[open_end..close_start].trim().is_empty();
            if body_is_empty {
                // `{ }` opened up: the additions on lines of their own, and the brace closed on the
                // line its node began.
                let mut body = String::from("{\n");
                for (_, text) in &by_place {
                    body.push_str(text);
                }
                body.push_str(indent);
                body.push('}');
                let Some(open_start) = rel(open.span.0) else {
                    return Ok(None);
                };
                let Some(close_end) = rel(close.span.1) else {
                    return Ok(None);
                };
                replacements.push((open_start, close_end, body.trim_end_matches('\n').to_string()));
                // `}` ends the block's text; what follows in a close slice is not ours to keep.
                let close_tail = &full[close_start..close_end];
                if let Some(brace) = close_tail.find('}') {
                    if brace + 1 < close_tail.len() {
                        let last = replacements.last_mut().unwrap();
                        last.2.push_str(&close_tail[brace + 1..]);
                    }
                }
            } else {
                for (at, text) in by_place {
                    // Each on a line of its own, after the line of what it follows.
                    let line = format!("\n{}", text.trim_end_matches('\n'));
                    replacements.push((at, at, line));
                }
            }
        }

        if replacements.is_empty() {
            return Ok(Some(None));
        }
        // Applied from the end, so the spans before each one stay where they were; insertions at
        // one place keep the order they were made in.
        let mut order: Vec<usize> = (0..replacements.len()).collect();
        order.sort_by(|a, b| {
            replacements[*b]
                .0
                .cmp(&replacements[*a].0)
                .then_with(|| b.cmp(a))
        });
        let mut text = full.clone();
        for i in order {
            let (s, e, ref with) = replacements[i];
            text.replace_range(s..e, with);
        }
        Ok(Some(Some(text.replace("\r\n", "\n"))))
    }

    /// What changed in a node's own line since it was written - its header's parameters and its
    /// value - as replacements in its text. `None` when the text cannot say where.
    fn own_line_changes(
        node: &OverseerNode,
        snap: &NodeSourceSnapshot,
        len: usize,
    ) -> Option<Vec<(usize, usize, String)>> {
        let base = snap.span.0;
        let rel = |abs: usize| abs.checked_sub(base).filter(|r| *r <= len);
        let mut replacements = Vec::new();
        // The header's parameters, when what they say has changed.
        let now = Self::render_params(node);
        let was = snap.header.parameters.as_ref();
        let said = was.map(|slice| Self::params_said(&slice.text));
        let current = Self::params_now(node);
        if said.as_ref() != Some(&current) && !(said.is_none() && current.is_empty()) {
            match (was, now) {
                (Some(slice), now) => {
                    let (Some(s), Some(e)) = (rel(slice.span.0), rel(slice.span.1)) else {
                        return None;
                    };
                    replacements.push((s, e, now.unwrap_or_default()));
                }
                (None, Some(now)) => {
                    let after = snap
                        .header
                        .name
                        .as_ref()
                        .or(snap.header.type_token.as_ref())
                        .or(snap.header.template.as_ref());
                    let Some(at) = after.and_then(|slice| rel(slice.span.1)) else {
                        return None;
                    };
                    replacements.push((at, at, format!(" {}", now)));
                }
                (None, None) => {}
            }
        }
        // The value, when it was written here and is no longer what was written.
        if let (Some(slice), Some(value)) = (snap.body.value.as_ref(), node.parameters.get("value")) {
            if !node.parameters.contains_key("_template_value") {
                let text = Self::serialize_value_with_node(node, value);
                if text != slice.text {
                    let (Some(s), Some(e)) = (rel(slice.span.0), rel(slice.span.1)) else {
                        return None;
                    };
                    replacements.push((s, e, text));
                }
            }
        }
        Some(replacements)
    }

    /// A list an instance wrote, with its entries given back from their own text: each one that
    /// changed spliced in where it stands, and one added since put after the entry before it.
    /// `None` when the entries no longer stand in the order they were written, or one was taken
    /// out - the list is then written as any list is.
    fn rewrite_list(
        node: &OverseerNode,
        snap: &NodeSourceSnapshot,
        indent: &str,
        unit: &str,
    ) -> Result<Option<Option<String>>> {
        // Spliced in the text as it was read, whose spans these are; line ends are evened out
        // once the text is whole.
        let full = snap.full_text.to_string();
        let base = snap.span.0;
        let rel = |abs: usize| abs.checked_sub(base).filter(|r| *r <= full.len());
        let (Some(open), Some(close)) = (
            snap.body.child_envelope.open.as_ref(),
            snap.body.child_envelope.close.as_ref(),
        ) else {
            return Ok(None);
        };
        let (Some(open_end), Some(close_start)) = (rel(open.span.1), rel(close.span.0)) else {
            return Ok(None);
        };
        let Some(mut replacements) = Self::own_line_changes(node, snap, full.len()) else {
            return Ok(None);
        };
        let written_from = |child: &OverseerNode| {
            Self::snapshot_for(child)
                .filter(|s| matches!(s.origin, SnapshotOrigin::Parsed))
                .and_then(|s| Some((rel(s.span.0)?, rel(s.span.1)?)))
        };
        let entry_indent = node
            .children
            .iter()
            .find_map(|c| written_from(c))
            .map(|(s, _)| Self::line_indent_at(&full, s))
            .unwrap_or_else(|| format!("{}{}", indent, unit));
        let Some(close_brace) = full[close_start..].find('}').map(|at| close_start + at) else {
            return Ok(None);
        };
        // The text an entry has now: its own, with what changed spliced in, or one added since
        // written as any entry is.
        let entry_text = |child: &OverseerNode, span: Option<(usize, usize)>| -> Result<Option<String>> {
            match span {
                Some((s, e)) => Ok(match Self::rewrite_written(child, &entry_indent, unit)? {
                    None => None,
                    Some(None) => Some(full[s..e].trim_end().to_string()),
                    Some(Some(text)) => Some(text.trim_end().to_string()),
                }),
                None => {
                    let mut text = String::from("\n");
                    let level = entry_indent.len() / unit.len().max(1);
                    Self::serialize_node_context(child, &mut text, level, true, unit, None, false)?;
                    Ok(Some(text.trim().to_string()))
                }
            }
        };
        let spans: Vec<Option<(usize, usize)>> = node.children.iter().map(|c| written_from(c)).collect();
        let written: Vec<(usize, usize)> = spans.iter().flatten().copied().collect();
        let in_order = written.windows(2).all(|pair| pair[0].1 <= pair[1].0);
        if !in_order {
            // Moved or sorted: the entries in their new order, each as its own text.
            let mut body = String::new();
            for (child, span) in node.children.iter().zip(spans.iter()) {
                let Some(text) = entry_text(child, *span)? else {
                    return Ok(None);
                };
                body.push_str(&format!("\n{}{}", entry_indent, text));
            }
            body.push_str(&format!("\n{}", indent));
            replacements.push((open_end, close_brace, body));
            let mut text = full.clone();
            replacements.sort_by(|a, b| b.0.cmp(&a.0));
            for (s, e, with) in replacements {
                text.replace_range(s..e, &with);
            }
            return Ok(Some(Some(text.replace("\r\n", "\n"))));
        }
        // What lies between two entries is layout or a comment, unless an entry that stood there
        // has been taken out: then only the layout before the next one stays.
        let closing_gap = |from: usize, to: usize| -> Option<(usize, usize, String)> {
            let gap = &full[from..to];
            if Self::holds_only_trivia(gap) {
                return None;
            }
            let kept = gap.rfind('\n').map(|at| gap[at..].to_string()).unwrap_or_else(|| " ".to_string());
            Some((from, to, kept))
        };
        let mut previous_end = open_end;
        for (child, span) in node.children.iter().zip(spans.iter()) {
            match span {
                Some((s, e)) => {
                    let (s, e) = (*s, s + full[*s..*e].trim_end().len());
                    if let Some(removed) = closing_gap(previous_end, s) {
                        replacements.push(removed);
                    }
                    match Self::rewrite_written(child, &entry_indent, unit)? {
                        None => return Ok(None),
                        Some(None) => {}
                        Some(Some(text)) => replacements.push(Self::replacing(&full, s, e, &text)),
                    }
                    previous_end = e;
                }
                None => {
                    let Some(text) = entry_text(child, None)? else {
                        return Ok(None);
                    };
                    replacements.push((previous_end, previous_end, format!("\n{}{}", entry_indent, text)));
                }
            }
        }
        if let Some(removed) = closing_gap(previous_end, close_brace) {
            replacements.push(removed);
        }
        if previous_end == open_end && full[open_end..close_start].trim().is_empty() {
            // `{ }` given its first entries: opened up, and closed on the line the list began.
            let added: Vec<String> = replacements
                .iter()
                .filter(|(s, e, _)| *s == open_end && *e == open_end)
                .map(|(_, _, line)| line.clone())
                .collect();
            if !added.is_empty() {
                replacements.retain(|(s, e, _)| !(*s == open_end && *e == open_end));
                let (Some(open_start), Some(close_end)) = (rel(open.span.0), rel(close.span.1)) else {
                    return Ok(None);
                };
                let tail = full[close_start..close_end]
                    .find('}')
                    .map(|brace| full[close_start + brace + 1..close_end].to_string())
                    .unwrap_or_default();
                replacements.push((
                    open_start,
                    close_end,
                    format!("{{{}\n{}}}{}", added.concat(), indent, tail),
                ));
            }
        }
        if replacements.is_empty() {
            return Ok(Some(None));
        }
        let mut order: Vec<usize> = (0..replacements.len()).collect();
        order.sort_by(|a, b| {
            replacements[*b]
                .0
                .cmp(&replacements[*a].0)
                .then_with(|| b.cmp(a))
        });
        let mut text = full.clone();
        for i in order {
            let (s, e, ref with) = replacements[i];
            text.replace_range(s..e, with);
        }
        Ok(Some(Some(text.replace("\r\n", "\n"))))
    }

    /// `text` in place of what lies between `s` and `e`; nothing, with the line it stood on, when
    /// the node has gone.
    fn replacing(full: &str, s: usize, e: usize, text: &str) -> (usize, usize, String) {
        let text = text.trim_end();
        if !text.is_empty() {
            return (s, e, text.to_string());
        }
        let line_start = full[..s].rfind('\n').map(|at| at + 1).unwrap_or(0);
        if line_start > 0 && full[line_start..s].trim().is_empty() {
            let from = if full[..line_start - 1].ends_with('\r') {
                line_start - 2
            } else {
                line_start - 1
            };
            return (from, e, String::new());
        }
        (s, e, String::new())
    }

    /// A node no block wrote, as the override that says what was set on it since: `- name = value`
    /// for a field, the container named with the same again inside for one that holds edits, and
    /// a node made at runtime or a list given records written whole.
    ///
    /// Inside an instance a template holds, only what was marked as edited here is written
    /// (`only_marked`): the rest carries the template's own overrides.
    fn write_added(
        node: &OverseerNode,
        output: &mut String,
        indent: &str,
        unit: &str,
        only_marked: bool,
    ) -> Result<()> {
        let given = Self::is_template_derived(node) || Self::written_mark(node).is_some();
        if !given || node.node_type == "list" {
            let mut text = String::from("\n");
            Self::serialize_node_context(node, &mut text, indent.len() / unit.len().max(1), false, unit, None, false)?;
            output.push_str(indent);
            output.push_str(text.trim_start());
            if !output.ends_with('\n') {
                output.push('\n');
            }
            return Ok(());
        }
        let mut anchor = None;
        let mut inner = Vec::new();
        let only_marked = only_marked || Self::is_a_nested_instance(node);
        if only_marked {
            Self::gather_marked(&node.children, &mut inner);
        } else {
            Self::gather_block(&node.children, 0, true, false, &mut anchor, &mut inner);
        }
        output.push_str(indent);
        // A div says what it is, as an entry already writes one; anything else is matched by name.
        if node.node_type == "div" && !inner.is_empty() {
            output.push_str(&format!("div {}", node.name));
        } else {
            output.push_str(&format!("- {}", node.name));
        }
        if let Some(params) = Self::render_params(node) {
            output.push(' ');
            output.push_str(&params);
        }
        if let (true, Some(value)) = (Self::value_set_since(node), node.parameters.get("value")) {
            output.push_str(&format!(" = {}", Self::serialize_value_with_node(node, value)));
        }
        if inner.is_empty() {
            output.push('\n');
            return Ok(());
        }
        output.push_str(" {\n");
        let deeper = format!("{}{}", indent, unit);
        for piece in inner {
            if let BlockPiece::Added(_, child) = piece {
                Self::write_added(child, output, &deeper, unit, only_marked)?;
            }
        }
        output.push_str(indent);
        output.push_str("}\n");
        Ok(())
    }

    /// The parameters a header's text says, by name, as the parser reads them.
    fn params_said(text: &str) -> Vec<(String, OverseerValue)> {
        let probe = format!("div probe {} {{\n}}\n", text);
        let Ok((_, nodes)) = crate::parser::parse_document(&probe) else {
            return Vec::new();
        };
        let Some(node) = nodes.first() else {
            return Vec::new();
        };
        let mut said: Vec<(String, OverseerValue)> = node
            .parameters
            .iter()
            .filter(|(k, _)| !k.starts_with('_'))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        said.sort_by(|a, b| a.0.cmp(&b.0));
        said
    }

    fn params_now(node: &OverseerNode) -> Vec<(String, OverseerValue)> {
        let mut now: Vec<(String, OverseerValue)> = Self::collect_serializable_params(node)
            .into_iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        now.sort_by(|a, b| a.0.cmp(&b.0));
        now
    }

    fn try_inline_patch_snapshot(
        node: &OverseerNode,
        snapshot: &NodeSourceSnapshot,
    ) -> Option<String> {
        // A list's entries keep the text they were read from, and a list that has lost one since
        // still holds it in its own: patched, the entry taken out came back. Its entries are
        // written one by one instead, each from its own text.
        if node.node_type == "list" && node.children.iter().any(Self::is_a_written_instance) {
            return None;
        }
        for (idx, child) in node.children.iter().enumerate() {
            if let Some(original_idx) = child.child_original_index {
                if original_idx != idx {
                    return None;
                }
            } else if child.source_fingerprint.is_none() {
                return None;
            }
        }

        let mut replacements: Vec<(usize, usize, String)> = Vec::new();
        let mut any_changed = false;
        let mut text_search_offset: usize = 0;

        for (idx, child) in node.children.iter().enumerate() {
            let child_snap = match Self::snapshot_for(child) {
                Some(s) => s,
                None => {
                    return None;
                }
            };
            let child_changed = match child.source_fingerprint {
                Some(fp) => {
                    if fp != child_snap.fingerprint {
                        true
                    } else {
                        !Self::children_match_snapshots(child)
                    }
                }
                None => true,
            };
            if !child_changed {
                continue;
            }

            let span_result = if child_snap.span.0 == 0 && child_snap.span.1 == 0 {
                match Self::locate_child_span_by_text(snapshot, &child_snap.full_text, text_search_offset) {
                    Some((start, end)) => (start, end),
                    None => {
                        if matches!(child_snap.origin, SnapshotOrigin::Synthetic(_)) {
                            return None;
                        }
                        return None;
                    }
                }
            } else {
                let start = match child_snap.span.0.checked_sub(snapshot.span.0) {
                    Some(val) => val,
                    None => {
                        return None;
                    }
                };
                let end = match child_snap.span.1.checked_sub(snapshot.span.0) {
                    Some(val) => val,
                    None => {
                        return None;
                    }
                };
                (start, end)
            };
            let (rel_start, rel_end) = match span_result {
                (start, end) => (start, end),
            };
            if rel_start >= rel_end || rel_end > snapshot.full_text.len() {
                return None;
            }

            if let Some(replacement_text) =
                Self::rebuild_child_with_updated_value(&node.children[idx], &child_snap)
            {
                replacements.push((rel_start, rel_end, replacement_text));
                text_search_offset = rel_end.max(text_search_offset);
                any_changed = true;
                continue;
            }

            let rendered = if let Some(simple) = Self::serialize_inline_simple_field(&node.children[idx]) {
                simple
            } else if let Some(patched_child) = Self::try_inline_patch_snapshot(&node.children[idx], &child_snap) {
                patched_child
            } else {
                return None;
            };
            let original_segment = &snapshot.full_text[rel_start..rel_end];
            let mut leading_end = 0;
            for (byte_idx, ch) in original_segment.char_indices() {
                if ch.is_whitespace() {
                    leading_end = byte_idx + ch.len_utf8();
                    continue;
                }
                break;
            }
            let leading = &original_segment[..leading_end];

            let mut tail_start = original_segment.len();
            for (byte_idx, ch) in original_segment.char_indices().rev() {
                if !ch.is_whitespace() {
                    tail_start = byte_idx + ch.len_utf8();
                    break;
                }
            }
            let trailing = &original_segment[tail_start..];

            let mut replacement = rendered;
            let mut replacement_leading_trim = 0;
            for (byte_idx, ch) in replacement.char_indices() {
                if ch.is_whitespace() {
                    replacement_leading_trim = byte_idx + ch.len_utf8();
                    continue;
                }
                break;
            }
            if replacement_leading_trim > 0 {
                replacement.drain(..replacement_leading_trim);
            }

            let trimmed_len = replacement
                .trim_end_matches(|c: char| c.is_whitespace())
                .len();
            replacement.truncate(trimmed_len);

            if !leading.is_empty() {
                replacement.insert_str(0, leading);
            }
            if !trailing.is_empty() {
                replacement.push_str(trailing);
            }
            replacements.push((rel_start, rel_end, replacement));
            text_search_offset = rel_end.max(text_search_offset);
            any_changed = true;
        }

        if !any_changed || replacements.is_empty() {
            return None;
        }

        replacements.sort_by(|a, b| b.0.cmp(&a.0));
        let mut patched_text = snapshot.full_text.to_string();
        for (start, end, replacement) in replacements {
            if replacement.contains('\n') && snapshot.full_text[start..end].contains("\r\n") {
                let normalized = replacement.replace('\n', "\r\n");
                patched_text.replace_range(start..end, &normalized);
            } else {
                patched_text.replace_range(start..end, &replacement);
            }
        }
        Some(patched_text)
    }

    fn locate_child_span_by_text(
        snapshot: &NodeSourceSnapshot,
        child_text: &str,
        search_start: usize,
    ) -> Option<(usize, usize)> {
        if child_text.is_empty() {
            return None;
        }
        if search_start > snapshot.full_text.len() {
            return None;
        }
        let haystack = &snapshot.full_text[search_start..];
        match haystack.find(child_text) {
            Some(pos) => {
                let abs_start = search_start + pos;
                let abs_end = abs_start + child_text.len();
                Some((abs_start, abs_end))
            }
            None => None,
        }
    }

    fn collect_descendant_inline_replacements(
        parent_snapshot: &NodeSourceSnapshot,
        node: &OverseerNode,
        replacements: &mut Vec<(usize, usize, String)>,
    ) -> Option<usize> {
        let mut max_end: Option<usize> = None;
        for child in &node.children {
            Self::collect_descendant_inline_replacements_inner(
                parent_snapshot,
                child,
                replacements,
                &mut max_end,
            );
        }
        max_end
    }

    fn collect_descendant_inline_replacements_inner(
        parent_snapshot: &NodeSourceSnapshot,
        node: &OverseerNode,
        replacements: &mut Vec<(usize, usize, String)>,
        max_end: &mut Option<usize>,
    ) {
        let Some(node_snap) = Self::snapshot_for(node) else {
            return;
        };

        let changed = match node.source_fingerprint {
            Some(fp) => fp != node_snap.fingerprint,
            None => true,
        };

        let has_real_span = node_snap.span.1 > node_snap.span.0;
        let within_parent = node_snap.span.0 >= parent_snapshot.span.0
            && node_snap.span.1 <= parent_snapshot.span.1;

        if changed && has_real_span && within_parent {
            let parent_start = parent_snapshot.span.0;
            let Some(rel_start) = node_snap.span.0.checked_sub(parent_start) else {
                return;
            };
            let Some(rel_end) = node_snap.span.1.checked_sub(parent_start) else {
                return;
            };
            if rel_start >= rel_end || rel_end > parent_snapshot.full_text.len() {
                return;
            }

            let rendered = if let Some(simple) = Self::serialize_inline_simple_field(node) {
                Some(simple)
            } else {
                Self::try_inline_patch_snapshot(node, &node_snap)
            };

            if let Some(mut replacement_body) = rendered {
                let original_segment = &parent_snapshot.full_text[rel_start..rel_end];

                let mut leading_end = 0;
                for (byte_idx, ch) in original_segment.char_indices() {
                    if ch.is_whitespace() {
                        leading_end = byte_idx + ch.len_utf8();
                        continue;
                    }
                    break;
                }
                let leading = &original_segment[..leading_end];

                let mut tail_start = original_segment.len();
                for (byte_idx, ch) in original_segment.char_indices().rev() {
                    if !ch.is_whitespace() {
                        tail_start = byte_idx + ch.len_utf8();
                        break;
                    }
                }
                let trailing = &original_segment[tail_start..];

                let mut replacement_leading_trim = 0;
                for (byte_idx, ch) in replacement_body.char_indices() {
                    if ch.is_whitespace() {
                        replacement_leading_trim = byte_idx + ch.len_utf8();
                        continue;
                    }
                    break;
                }
                if replacement_leading_trim > 0 {
                    replacement_body.drain(..replacement_leading_trim);
                }

                let trimmed_len = replacement_body
                    .trim_end_matches(|c: char| c.is_whitespace())
                    .len();
                replacement_body.truncate(trimmed_len);

                if !leading.is_empty() {
                    replacement_body.insert_str(0, leading);
                }
                if !trailing.is_empty() {
                    replacement_body.push_str(trailing);
                }

                replacements.push((rel_start, rel_end, replacement_body));
                let current_max = max_end.unwrap_or(0);
                *max_end = Some(current_max.max(rel_end));
                return; // Do not recurse into children once this node is replaced
            }

        }

        for child in &node.children {
            Self::collect_descendant_inline_replacements_inner(
                parent_snapshot,
                child,
                replacements,
                max_end,
            );
        }
    }

    fn children_match_snapshots(node: &OverseerNode) -> bool {
        node.children.iter().all(|child| {
            if let Some(child_snap) = Self::snapshot_for(child) {
                match child.source_fingerprint {
                    Some(fp) if fp == child_snap.fingerprint => Self::children_match_snapshots(child),
                    _ => false,
                }
            } else {
                false
            }
        })
    }

    fn collect_serializable_params(node: &OverseerNode) -> Vec<(&String, &OverseerValue)> {
        let mut params: Vec<(&String, &OverseerValue)> = Vec::new();
        for (key, value) in &node.parameters {
            let key_str = key.as_str();
            if key_str == "value"
                || key_str == "_original_type"
                || (key_str.starts_with('_') && !key_str.starts_with("_template_"))
            {
                continue;
            }
            if key_str.starts_with("_template_") {
                continue;
            }

            let template_marker = format!("_template_{}", key_str);
            if node.parameters.contains_key(&template_marker) {
                continue;
            }

            params.push((key, value))
        }
        // Parameters live in a HashMap, so their iteration order is meaningless. Sorting by
        // name at least makes output deterministic, but it also rewrites every declaration
        // the moment a node is reformatted - `(source=..., lazy=false)` comes back as
        // `(lazy=false, source=...)`. The parser records the authored order for exactly this
        // reason; anything it did not record keeps the alphabetical fallback.
        params.sort_by(|a, b| {
            let rank = |key: &String| {
                node.param_order
                    .iter()
                    .position(|k| k == key)
                    .unwrap_or(usize::MAX)
            };
            rank(a.0).cmp(&rank(b.0)).then_with(|| a.0.cmp(b.0))
        });
        params
    }

    /// Whether everything under this node came with it from the template, so that writing it as
    /// a value alone loses nothing.
    ///
    /// A value written to a field the template gives a body - `enum stage = "filed" { on change
    /// {...} }` - is still only a value. The body is the template's to say, and every entry has it
    /// without stating it. Asking only whether there were children wrote the override out as
    /// `enum stage = "ready" {}`, a declaration of its own, as soon as a stage was picked on an
    /// entry that had not stated one.
    fn has_only_the_templates_body(node: &OverseerNode) -> bool {
        node.children.iter().all(|c| {
            (matches!(c.parameters.get("_template_node"), Some(OverseerValue::Boolean(true)))
                || c.parameters.keys().any(|k| Self::marks_a_template_node(k)))
                && !matches!(
                    c.parameters.get("_explicit_child_override"),
                    Some(OverseerValue::Boolean(true))
                )
        })
    }

    fn serialize_inline_simple_field(node: &OverseerNode) -> Option<String> {
        // See `has_only_the_templates_body`. Only for an override: a node that is no override
        // and has children is a declaration, whoever supplied them.
        if !node.children.is_empty() {
            let overridden = matches!(
                node.parameters.get("_explicit_child_override"),
                Some(OverseerValue::Boolean(true))
            );
            if !overridden || !Self::has_only_the_templates_body(node) {
                return None;
            }
            if let Some(value) = node.parameters.get("value") {
                if !node.name.is_empty() && node.name != "-" {
                    let rendered_value = Self::serialize_value_with_node(node, value);
                    return Some(format!("- {} = {}", node.name, rendered_value));
                }
            }
            return None;
        }

        if let Some(snapshot) = Self::snapshot_for(node) {
            if let Some(body_text) = Self::snapshot_block_inner(&snapshot) {
                if !body_text.trim().is_empty() {
                    return None;
                }
            }
        }

        if let Some(value) = node.parameters.get("value") {
            let is_template_override = matches!(
                node.parameters.get("_template_node"),
                Some(OverseerValue::Boolean(true))
            ) || matches!(
                node.parameters.get("_explicit_child_override"),
                Some(OverseerValue::Boolean(true))
            ) || node.parameters.contains_key("_template_value");

            if is_template_override && !node.name.is_empty() && node.name != "-" {
                let rendered_value = Self::serialize_value_with_node(node, value);
                return Some(format!("- {} = {}", node.name, rendered_value));
            }
        }

        let mut out = String::new();
        if let Some(template_path) = &node.template {
            out.push_str(&format!("<{}>", template_path));
        } else if let Some(OverseerValue::Template(tpl)) = node.parameters.get("_template_origin")
        {
            out.push_str(&format!("<{}>", tpl));
        } else if let Some(OverseerValue::String(original_type)) =
            node.parameters.get("_original_type")
        {
            if original_type == "-" {
                out.push('-');
            } else {
                out.push_str(original_type);
            }
        } else if node.node_type == "-"
            || (node.name == "-" && node.node_type != "list_item")
        {
            out.push('-');
        } else {
            out.push_str(&node.node_type);
        }

        if !node.name.is_empty() && node.name != "-" {
            let auto_defaulted = node.name == node.node_type;
            if !auto_defaulted {
                out.push(' ');
                out.push_str(&node.name);
            }
        }

        let regular_params = Self::collect_serializable_params(node);
        if !regular_params.is_empty() {
            out.push_str(" (");
            let params_str: Vec<String> = regular_params
                .iter()
                .map(|(k, v)| {
                    let value_str = if k.as_str() == "entry" {
                        match v {
                            OverseerValue::String(s) => s.clone(),
                            OverseerValue::Template(t) => format!("<{}>", t),
                            other => Self::serialize_value_with_node(node, other),
                        }
                    } else {
                        Self::serialize_value_with_node(node, v)
                    };
                    format!("{}={}", k, value_str)
                })
                .collect();
            out.push_str(&params_str.join(", "));
            out.push(')');
        }

        if let Some(val) = node.parameters.get("value") {
            out.push_str(" = ");
            out.push_str(&Self::serialize_value_with_node(node, val));
        }

        // A childless node may still have been authored with an empty block - `div x { }`,
        // or a mount whose body is empty by definition. Nothing above emits braces, so
        // without this the node comes back as a bare declaration and the braces are gone.
        // It still parses, which is why the loss went unnoticed and compounded over saves.
        if let Some(snapshot) = Self::snapshot_for(node) {
            if let Some(block) = Self::snapshot_block_verbatim(&snapshot) {
                let spacer = if snapshot.header.trailing.is_empty() {
                    " ".to_string()
                } else {
                    snapshot.header.trailing.replace("\r\n", "\n")
                };
                out.push_str(&spacer);
                out.push_str(&block);
            }
        }

        Some(out)
    }

    fn rebuild_child_with_updated_value(
        node: &OverseerNode,
        snapshot: &NodeSourceSnapshot,
    ) -> Option<String> {
        let value_slice = snapshot.body.value.as_ref()?;
        let value_param = node.parameters.get("value")?;

        let rel_start = value_slice.span.0.checked_sub(snapshot.span.0)?;
        let rel_end = value_slice.span.1.checked_sub(snapshot.span.0)?;
        if rel_start >= rel_end {
            return None;
        }

        if rel_end > snapshot.full_text.len() {
            return None;
        }

        let mut updated_text = snapshot.full_text.to_string();
        if rel_end > updated_text.len() {
            return None;
        }

        let new_value = Self::serialize_value_with_node(node, value_param);
        updated_text.replace_range(rel_start..rel_end, &new_value);

        Some(updated_text)
    }

    fn reconstruct_inline_segment(
        node: &OverseerNode,
        snapshot: &NodeSourceSnapshot,
        inline_start_abs: usize,
        inline_end_abs: usize,
        inline_child_records: &[(usize, std::sync::Arc<NodeSourceSnapshot>)],
    ) -> Option<String> {
        if inline_child_records.is_empty() {
            return None;
        }

        let doc_start = snapshot.span.0;
        if inline_start_abs < doc_start || inline_end_abs > doc_start + snapshot.full_text.len() {
            return None;
        }
        let mut cursor = inline_start_abs - doc_start;
        let inline_end_rel = inline_end_abs - doc_start;
        let mut result = String::new();

        for (idx, child_snapshot) in inline_child_records {
            let child_rel_start = child_snapshot.span.0.checked_sub(doc_start)?;
            let child_rel_end = child_snapshot.span.1.checked_sub(doc_start)?;
            if child_rel_end > inline_end_rel || child_rel_start < cursor {
                return None;
            }

            if child_rel_start > snapshot.full_text.len() || child_rel_end > snapshot.full_text.len()
            {
                return None;
            }

            let prefix = &snapshot.full_text[cursor..child_rel_start];
            result.push_str(&prefix.replace("\r\n", "\n"));

            let child_render = Self::serialize_inline_simple_field(&node.children[*idx])?;
            result.push_str(&child_render);

            cursor = child_rel_end;
        }

        if cursor <= inline_end_rel && inline_end_rel <= snapshot.full_text.len() {
            let suffix = &snapshot.full_text[cursor..inline_end_rel];
            result.push_str(&suffix.replace("\r\n", "\n"));
        }

        Some(result)
    }

    /// A string as a document may hold it: on one line, and readable back as itself.
    ///
    /// Without this, prose destroys the document it is written into. A recap containing a
    /// quotation mark ended the value early and left the rest of the sentence as stray tokens;
    /// one containing a newline spread a value across lines that the next field then landed
    /// inside. Both cost the whole entry, and the failure appears as a document that no longer
    /// parses rather than as anything to do with the text that was written.
    ///
    /// Escaped rather than rejected: a diary is prose, and prose has quotation marks in it.
    fn escape_string(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ => out.push(c),
            }
        }
        out
    }

    pub(crate) fn serialize_value(value: &OverseerValue) -> String {
        match value {
            OverseerValue::Null => "null".to_string(),
            OverseerValue::String(s) => format!("\"{}\"", Self::escape_string(s)),
            OverseerValue::Integer(i) => i.to_string(),
            OverseerValue::Float(f) => crate::numbers::write_float(*f),
            OverseerValue::Boolean(b) => b.to_string(),
            OverseerValue::Date(d) => format!("\"{}\"", d),
            OverseerValue::Timestamp(ts) => format!("\"{}\"", ts),
            OverseerValue::Formula(f) => format!("$({})", f),
            OverseerValue::Template(t) => format!("<{}>", t),
            OverseerValue::Color(color) => match color {
                Color::Hex(hex) => hex.clone(),
                Color::Named(name) => name.clone(),
                Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
            },
            OverseerValue::CssSize(size) => match size {
                CssSize::Pixels(px) => format!("{}px", px),
                CssSize::Percentage(pct) => format!("{}%", pct),
                CssSize::Em(em) => format!("{}em", em),
                CssSize::Rem(rem) => format!("{}rem", rem),
                CssSize::ViewportWidth(vw) => format!("{}vw", vw),
                CssSize::ViewportHeight(vh) => format!("{}vh", vh),
                CssSize::Auto => "auto".to_string(),
                CssSize::FitContent => "fit-content".to_string(),
                CssSize::Fraction(v) => format!("{}fr", v),
            },
            OverseerValue::BorderStyle(style) => match style {
                BorderStyle::None => "none".to_string(),
                BorderStyle::Default => "default".to_string(),
                BorderStyle::Solid(thickness, color) => {
                    format!(
                        "solid {} {}",
                        match thickness {
                            CssSize::Pixels(px) => format!("{}px", px),
                            CssSize::Percentage(pct) => format!("{}%", pct),
                            CssSize::Em(em) => format!("{}em", em),
                            CssSize::Rem(rem) => format!("{}rem", rem),
                            CssSize::ViewportWidth(vw) => format!("{}vw", vw),
                            CssSize::ViewportHeight(vh) => format!("{}vh", vh),
                            CssSize::Auto => "auto".to_string(),
                            CssSize::FitContent => "fit-content".to_string(),
                            CssSize::Fraction(v) => format!("{}fr", v),
                        },
                        match color {
                            Color::Hex(hex) => hex.clone(),
                            Color::Named(name) => name.clone(),
                            Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
                        }
                    )
                }
                BorderStyle::Dashed(thickness, color) => {
                    format!(
                        "dashed {} {}",
                        match thickness {
                            CssSize::Pixels(px) => format!("{}px", px),
                            CssSize::Percentage(pct) => format!("{}%", pct),
                            CssSize::Em(em) => format!("{}em", em),
                            CssSize::Rem(rem) => format!("{}rem", rem),
                            CssSize::ViewportWidth(vw) => format!("{}vw", vw),
                            CssSize::ViewportHeight(vh) => format!("{}vh", vh),
                            CssSize::Auto => "auto".to_string(),
                            CssSize::FitContent => "fit-content".to_string(),
                            CssSize::Fraction(v) => format!("{}fr", v),
                        },
                        match color {
                            Color::Hex(hex) => hex.clone(),
                            Color::Named(name) => name.clone(),
                            Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
                        }
                    )
                }
                BorderStyle::Dotted(thickness, color) => {
                    format!(
                        "dotted {} {}",
                        match thickness {
                            CssSize::Pixels(px) => format!("{}px", px),
                            CssSize::Percentage(pct) => format!("{}%", pct),
                            CssSize::Em(em) => format!("{}em", em),
                            CssSize::Rem(rem) => format!("{}rem", rem),
                            CssSize::ViewportWidth(vw) => format!("{}vw", vw),
                            CssSize::ViewportHeight(vh) => format!("{}vh", vh),
                            CssSize::Auto => "auto".to_string(),
                            CssSize::FitContent => "fit-content".to_string(),
                            CssSize::Fraction(v) => format!("{}fr", v),
                        },
                        match color {
                            Color::Hex(hex) => hex.clone(),
                            Color::Named(name) => name.clone(),
                            Color::Rgb(r, g, b) => format!("rgb({}, {}, {})", r, g, b),
                        }
                    )
                }
            },
        }
    }

    // Variant of serialize_value that can look at the node's parameters to apply
    // precision-aware persistence for dates/timestamps (e.g., precision="day").
    fn serialize_value_with_node(node: &OverseerNode, value: &OverseerValue) -> String {
        // If this node indicates day precision, collapse Timestamp values to date-only.
        // We check both the node's own precision param and the common convention of field name 'date'.
        let day_precision = match node.parameters.get("precision") {
            Some(OverseerValue::String(s)) => {
                s.eq_ignore_ascii_case("day") || s.eq_ignore_ascii_case("days")
            }
            _ => false,
        };

        if day_precision {
            match value {
                OverseerValue::Timestamp(ts) => {
                    // Best-effort: extract YYYY-MM-DD from RFC3339; fall back to ts
                    let date_only = ts.split('T').next().unwrap_or(ts);
                    return format!("\"{}\"", date_only.to_string());
                }
                OverseerValue::Date(d) => {
                    // Already date-only; emit as-is quoted
                    return format!("\"{}\"", d);
                }
                _ => { /* fall through */ }
            }
        }
        // Default behavior
        Self::serialize_value(value)
    }
}

// Public facade used by other modules and tests
/// Set on a node by whatever writes to it - the page's edit and an action's - and never by the
/// template's text: a template's own overrides carry every other mark an edit carries, and
/// copying a template takes it off. It is what says an edit inside an instance a template holds
/// is the entry's own.
pub const EDITED_HERE: &str = "_edited_here";

pub struct OverseerFileHandler;

impl OverseerFileHandler {
    // Delegate to FileOperations' serializer
    pub fn serialize_nodes(nodes: &[OverseerNode]) -> Result<String> {
        FileOperations::serialize_nodes(nodes)
    }
}

/// One piece of an instance's block, as the writer gives the block back: an element the block
/// wrote, at its place in the block, or a node set since, after the written element before it.
enum BlockPiece<'a> {
    Written(usize, &'a OverseerNode),
    Added(Option<usize>, &'a OverseerNode),
}

#[cfg(test)]
mod tests_serializer_comment_trivia {
    use super::*;

    fn parse_and_resolve(src: &str) -> Vec<OverseerNode> {
        let (_rem, mut nodes) = crate::parser::parse_document(src).expect("parse");
        crate::resolver::resolve_document(&mut nodes);
        nodes
    }

    #[test]
    fn field_update_preserves_comments() {
        let _guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let original = r#"// Header line 1
// Header line 2

div T {
    // comment before field A
    int A = 1 // inline A

    // group comment
    int B = 2
}

// Standalone comment before instance
<T> I {
    - A = 3 // override inline
}

// Trailing file comment
"#;

        let mut nodes = parse_and_resolve(original);
        let div_t = nodes
            .iter_mut()
            .find(|n| n.node_type == "div" && n.name == "T")
            .expect("div T not found");
        let field_b = div_t
            .children
            .iter_mut()
            .find(|child| child.name == "B")
            .expect("field B not found");
        field_b
            .parameters
            .insert("value".to_string(), OverseerValue::Integer(42));
        field_b.source_fingerprint = None;

        let output = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(output.contains("// Header line 1"));
        assert!(output.contains("// comment before field A"));
        assert!(output.contains("// inline A"));
        assert!(output.contains("// Standalone comment before instance"));
        assert!(output.trim_end().ends_with("// Trailing file comment"));
        assert!(output.contains("int B = 42"));
    }

    #[test]
    fn list_comment_survives_entry_change() {
        let _guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let original = r#"div T {
    int A = 1
    int B = 2
    int C = 3
}
<T> I {
    - A = 3
    - B = 2
}

// List as an example of correct behavior:
list L(entry=<T>) {
    - {
        - A = 3
        - B = 2
    }
}
"#;

        let mut nodes = parse_and_resolve(original);
        let list_l = nodes
            .iter_mut()
            .find(|n| n.node_type == "list" && n.name == "L")
            .expect("list L not found");
        list_l.source_fingerprint = None;
        let entry = list_l.children.first_mut().expect("entry not found");
        entry.source_fingerprint = None;
        if let Some(field_b) = entry.children.iter_mut().find(|child| child.name == "B") {
            field_b
                .parameters
                .insert("value".to_string(), OverseerValue::Integer(5));
            field_b.source_fingerprint = None;
        }

    let output = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(output.contains("// List as an example of correct behavior:"));
        assert!(output.contains("list L (entry=<T>)"));
        assert!(output.contains("- B = 5"));
    }

    #[test]
    fn empty_block_comment_preserved_after_rename() {
        let _guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let original = r#"list FinalTemplate (entry=<ExtendedTemplate>) {
    - {
        // Should inherit BaseTemplate parameters through ExtendedTemplate
    }
}
"#;

        let mut nodes = parse_and_resolve(original);
        let list = nodes
            .iter_mut()
            .find(|n| n.node_type == "list" && n.name == "FinalTemplate")
            .expect("list not found");
        list.source_fingerprint = None;
        let entry = list.children.first_mut().expect("entry not found");
        entry.name = "Entry1".to_string();
        entry.source_fingerprint = None;

        let output = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(
            output.contains("// Should inherit BaseTemplate parameters through ExtendedTemplate")
        );
        assert!(output.contains("Entry1"));
    }

    #[test]
    fn chart_comment_survives_plot_update() {
        let _guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let original = r##"chart C {
    plot P1 (color="#ff0000", label="A", source=$(foo), x=$(bar), y=$(baz))
    // P3: cumulative average over all exercises per day for each point's day
    plot P3 (color="#b42c22ff", label="Exercises", source=$(/drum_tracker/History.filter(|x| x/eid == 98) ), x=$(|t| t/time), y=$(|t| /drum_tracker/History.filter(|x| same_day(x/time, t/time)&&(x/eid!=33 && x/eid!=31 )).average(|x| x/avg_points)))
    plot P3 (color="#792eabff", label="Syncopated", source=$(/drum_tracker/History.filter(|x| x/eid == 80) ), x=$(|t| t/time), y=$(|t| /drum_tracker/History.filter(|x| same_day(x/time, t/time)&&(x/eid==80||x/eid==84)).average(|x| x/avg_points)))
}
"##;

        let mut nodes = parse_and_resolve(original);
        let chart = nodes
            .iter_mut()
            .find(|n| n.node_type == "chart" && n.name == "C")
            .expect("chart not found");
        chart.source_fingerprint = None;
        if let Some(plot) = chart.children.iter_mut().find(|child| child.name == "P1") {
            plot.parameters.insert(
                "color".to_string(),
                OverseerValue::String("#00ff00".to_string()),
            );
            plot.source_fingerprint = None;
        }

        let output = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");
        assert!(output
            .contains("// P3: cumulative average over all exercises per day for each point's day"));
        assert!(output.contains("color=\"#00ff00\""));
    }
}

#[cfg(test)]
mod tests_serialization_preserve_transparent_overrides {
    use super::*;

    #[test]
    fn preserve_field_overrides_inside_transparent_wrapper_in_list_entry() {
        let original = r#"div A {
    int field_a = 0
    div {
        int field_b = 0
    }
}

list L(entry=<A>) {
    - {
        - field_a = 10
        - field_b = 20
    }
}
"#;

        // Parse and resolve to simulate runtime processing (template clone + markers)
        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse");
        crate::resolver::resolve_document(&mut nodes);

        // Serialize back
        let serialized = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

        // Must keep concise override for field_b (no loss through unnamed wrapper)
        assert!(
            serialized.contains("- field_b = 20"),
            r#"Expected serialized content to contain "- field_b = 20" but got:
{}"#,
            serialized
        );
        // Round-trip: parse serialized and resolve; verify the first list entry sees field_b=20 via transparency
        let (_rem2, mut nodes2) = crate::parser::parse_document(&serialized).expect("parse2");
        crate::resolver::resolve_document(&mut nodes2);

        // Find list L, then its first entry, then accessible child 'field_b'
        let list = nodes2
            .iter()
            .find(|n| n.node_type == "list" && n.name == "L")
            .expect("list L not found");
        assert!(
            !list.children.is_empty(),
            "list L has no children after roundtrip: {}",
            serialized
        );
        let entry = &list.children[0];
        // After resolution, list entries are template-instantiated using the template name as node_type (A) and auto name like A__1
        let children = entry.get_accessible_children();
        let field_b = children
            .into_iter()
            .find(|c| c.name == "field_b")
            .expect("field_b not accessible in entry after roundtrip");
        let val = field_b
            .parameters
            .get("value")
            .cloned()
            .expect("field_b has no value after roundtrip");
        assert_eq!(
            val,
            OverseerValue::Integer(20),
            "field_b value mismatch: {:?}",
            val
        );
    }
}

#[cfg(test)]
mod tests_serialization_formatting {
    use super::*;

    fn strip_snapshots(nodes: &mut [OverseerNode]) {
        for node in nodes {
            node.source_snapshot = None;
            if !node.children.is_empty() {
                strip_snapshots(&mut node.children);
            }
        }
    }

    #[test]
    fn serializer_preserves_indent_and_newlines_via_registry() {
        let _registry_guard = crate::source_registry::REGISTRY_TEST_MUTEX.lock();
        let original = "tab Root {\r\n  string title = \"Hi\"\r\n\r\n  div Group {\r\n    int value = 1\r\n  }\r\n}\r\n";

        let (_rem, mut nodes) = crate::parser::parse_document(original).expect("parse");
        crate::resolver::resolve_document(&mut nodes);

        let root = nodes.first().expect("root node");
        assert_eq!(root.children.len(), 2, "expected two children under root");
        assert_eq!(
            root.children[0].leading_blank_lines, 1,
            "parser encodes one newline before first child"
        );
        assert_eq!(
            root.children[1].leading_blank_lines, 2,
            "parser encodes newline plus blank spacer before second child"
        );
        let group = &root.children[1];
        assert_eq!(group.children.len(), 1, "group should have one child");
        assert_eq!(
            group.children[0].leading_blank_lines, 1,
            "nested child records only the structural newline"
        );

        strip_snapshots(&mut nodes);

        let serialized = OverseerFileHandler::serialize_nodes(&nodes).expect("serialize");

        assert_eq!(
            serialized,
            original,
            "Serializer should retain CRLF newlines and two-space indentation fetched from SourceRegistry"
        );

        crate::source_registry::SourceRegistry::reset();
    }
}
