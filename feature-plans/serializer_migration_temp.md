# Goal: Serializer Migration
The old serializer rebuilds the entire document from the AST, which leads to several issues:
- Whitespace drift
- Parameter reordering
- Comment corruption
- Layout/indent changes even without edits
The goal is to shift serializer from generator to patcher. Target behavior examples:
- No-op round trips: parse → resolve → serialize should return the byte-identical original when no data or structure changed.
- Localized diffs: edits should only touch the affected subtree (e.g., edited value line, appended list entry) without reflowing unrelated sections.
- Style inheritance for new nodes: when cloning/appending list entries we should mimic surrounding trivia—indent width, inline vs multiline braces, spacing before/after comments.
- Comment/trivia stability: both inline and standalone comments (plus blank lines) must remain anchored to their original neighbors.

# Serializer Migration Plan (Temporary Tracking)

| Step | Description | Status | Notes |
| --- | --- | --- | --- |
| 1 | Input layer: span-aware parsing with trivia + node fingerprint | ✅ Completed (2025-10-02) | Parser now records leading/trailing trivia, header/body spans, and fingerprints for each node. |
| 2 | Data model: structured snapshots on nodes | ✅ Completed (2025-10-07) | Structured header/body snapshots (with template/type/name/params/value spans) flow through parser, resolver, and action paths; whitespace counters corrected for CRLF and template clones now defer indent to contextual fallback. |
| 3 | Serializer 2.0: snapshot-driven patcher | ⏳ Not started | Blocked on finalized snapshot schema. |
| 4 | Insert heuristics for list entry styling | ⏳ Not started | Depends on new serializer infrastructure. |
| 5 | Retire `merge_comments` | ⏳ Not started | Requires snapshot-driven comment handling. |
| 6 | Regression harness for serializer | ⏳ Not started | Set of golden fixtures once serializer rewrite stabilizes. |

## Recent updates
- Finalized `NodeSourceSnapshot` structure with dedicated header/body slices, child envelope tokens, and newline detection.
- Propagated snapshots (and fingerprints) through resolver/action cloning so template instances retain source provenance.
- Normalized blank-line accounting for CRLF trivia and taught template clones to fall back to contextual indentation, restoring comment merge regression tests.

## Next focus
1. Kick off Step 3: design serializer patcher that consumes structured snapshots and SourceRegistry fingerprints.
2. Introduce focused regression fixtures around list entry formatting to guard the new blank-line accounting.
3. Map remaining serializer heuristics (list styling, comment retirement) onto the new snapshot contract.
