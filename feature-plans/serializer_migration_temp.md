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
| 2 | Data model: structured snapshots on nodes | ✅ Completed (2025-10-03) | Structured header/body snapshots (with template/type/name/params/value spans) flow through parser, resolver, and action paths; whitespace counters corrected for CRLF and template clones now defer indent to contextual fallback. |
| 3 | Serializer 2.0: snapshot-driven patcher | ✅ Completed (2025-10-03) | Replays parsed snapshots with recursive fingerprint gating, newline normalization, and comment-safe merge fallbacks. |
| 4 | Insert heuristics for list entry styling | ✅ Completed (2025-10-03) | List append/prepend actions now derive local newline/indent guides and harmonize blank-line spacing to match existing entries. |
| 5 | Retire `merge_comments` | ✅ Completed (2025-10-03) | Serializer now replays snapshot trivia (leading + trailing) and list-entry bodies directly, making the merge path obsolete. |
| 6 | Regression harness for serializer | ⏳ Not started | Set of golden fixtures once serializer rewrite stabilizes. |

## Recent updates
- Snapshot patcher now replays unchanged subtrees directly from their stored snapshots, guarded by recursive fingerprint checks and override metadata so new edits keep their overrides.
- Serializer normalizes newline preferences and trims trailing trivia when replaying snapshots, tightening diff locality while honoring Windows CRLF inputs.
- Removed the legacy `merge_comments` fallback; inline and trailing trivia now come exclusively from `NodeSourceSnapshot`, including empty-block bodies and registry-level document tails.
- Template-derived list entries default to anonymous `- { ... }` emission, preserving concise overrides while keeping explicit names when authored.
- Added regression coverage for snapshot sibling preservation, toggle/append action overrides, and round-trip comment spacing.
- List append/prepend actions derive a `ListEntryStyleGuide` from parsed siblings, apply matching newline + indentation to inserted entries, and backfill blank-line spacing for existing children.

## Next focus
1. Step 6: assemble targeted golden fixtures to monitor diff size, comment retention, and CRLF fidelity across representative documents.
2. Investigate remaining property-suite gaps (e.g., example corpus comment counts) and fold fixes into the harness baseline.
