# Merge Indentation Structural Fix

This note documents the root cause and resolution for the sporadic indentation drift affecting list entry closing braces (`}`) when *prepending* a new list entry to an existing list.

## Symptom
After prepending a new `- { ... }` block at the start of a list, one of the *existing* list entry closing braces would occasionally lose its indentation (e.g. become column 0) even though only the new entry should differ. The synthetic test `prepend_duplicate_list_entry_preserves_existing_indentation` reproduced this reliably.

## Root Cause
The merge algorithm used positional + anchor based adoption of original formatting. Closing braces for list entries (`}`) share an identical trimmed anchor token and have no inline comment to disambiguate. When a new block is inserted at the top, relative positions shift and the occurrence queue mapping sometimes associated a later regenerated `}` with an earlier occurrence position (or skipped adoption entirely), causing the brace to pass through the *raw* output path with default indentation (none). Post‑pass indentation adoption considered that brace novel in context (or mismatched due to occurrence consumption offsets) and therefore did not correct it.

In short: `}` lines relied on anchor/position heuristics that are insufficiently discriminative when structurally identical braces shift due to prepends.

## Fix
Add a structural post‑pass rule: if a code line is a `}` and the nearest preceding code line is the start of a list entry `- {`, force the brace indentation to match the opening line's indentation. This bypasses ambiguous anchor matching entirely and guarantees identical indentation for each list entry's closing brace regardless of how many earlier siblings were prepended.

Implemented in `file_ops.rs` post‑pass loop (search for `list_entry_close_structural_fix`). This executes *before* generic adoption logic, ensuring braces are normalized early.

## Why Post‑Pass?
We already perform a final normalization / adoption sweep; centralizing the structural fix here avoids duplicating logic in three separate emission paths (strict match, relaxed match, raw). It also works on the fully collapsed output, simplifying context look‑back.

## Safety
The rule only triggers when:
1. Current trimmed code line is exactly `}`.
2. Nearest previous code line (ignoring blank/comment lines) is exactly `- {`.
3. Current indentation differs from the opening indentation.

This precisely targets list entry block closures and leaves all other braces (named blocks, nested containers) untouched.

## Result
The failing test now passes; all other tests remain green. Indentation drift for list entry braces after prepends is eliminated without altering existing comment or spacing preservation heuristics.

---
*Added: 2025-09-29*
