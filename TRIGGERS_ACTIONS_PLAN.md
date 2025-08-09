# Triggers & Actions System — Incremental Implementation Plan

This document outlines a stepwise plan to add a flexible triggers/actions system to Overseer, aligned with the project’s document-first, hierarchical DSL. Each increment is small, testable, and verified via unit/integration tests and UI smoke checks.

## Goals
- First-class “on … { … }” event blocks as children inside the document structure.
- Deterministic, transactional action execution with safe path resolution.
- Support list identity using list-defined keys (no UUIDs initially).
- Catch up time-based triggers on document load in correct order.
- Keep behavior composable (batch/if/for_each) and testable.

## Out-of-scope (initially)
- Background execution when the app is closed.
- Cross-document references beyond simple ensure_document (introduced later).
- Undo/redo (planned after action log foundation).

---

## Increment 0: DSL Surface and Parser Stubs

Deliverables
- Extend DSL grammar to support:
  - Event blocks: `on <eventName> { … }`
  - Action nodes as children inside `on` blocks (type = verb; params as key=value pairs)
- No runtime; parser only. Unknown actions are accepted but not executed yet.

Examples
```overseer
button Increment {
  on click { inc(path="../counter", by=1) }
}
```

Tests
- Parse event blocks and simple actions; produce AST nodes without errors.

---

## Increment 1: Action Engine Foundations (Transactions + Mutations)

Deliverables
- Backend action executor skeleton (Rust):
  - Transaction boundary per on block.
  - Mutation API: set, inc, toggle, clear, set_now (date/time).
  - Path resolution reuses formula semantics (computed-first reads, but writes target raw parameters/values).
- Re-run resolver + formula pass once per transaction.

Actions
- set(path=…, value=…)
- inc(path=…, by=1) / dec(path=…, by=1)
- toggle(path=…)
- clear(path=…)
- set_now(path=…, clock="utc|local")

Tests
- Unit tests for each mutation (happy + edge cases: non-numeric inc, missing path, boolean toggle).
- Integration: Counter button example (wired in Increment 2) increments value and persists across reevaluation.

---

## Increment 2: UI Event Wiring (click → backend)

Deliverables
- Frontend emits semantic UI events (node_id + eventName) for button clicks.
- Backend maps UI events to owning node, finds `on <eventName>` blocks, executes actions.

Tests
- UI smoke: Clicking a button with on click → inc updates adjacent int.
- No visual regressions (renderer unchanged aside from event emission hookup).

---

## Increment 3: List Identity via Key + List Mutations (MVP)

Deliverables
- Honor list key for identity: `list Tasks(entry=<Task>, key="task_id")`.
- Add list actions using key selectors:
  - append(list=…, template=<…> | value={…})
  - remove(from=/list, where=("keyField","keyValue"))
  - move(from=/list, where=("keyField","keyValue"), to=/targetList, at=index|afterKey=…)
  - ensure_in_list(list=/path, keyField="…", item={…} | template=<…>)

Tests
- Append/move/remove via key; idempotent ensure_in_list.
- Error paths: missing key field, duplicate key.

---

## Increment 4: Sorting and View Helpers

Deliverables
- sort(list=/path, by=|x| expr, order=asc|desc, stable=true)
- Optional non-mutating view helpers (UI-only): set_view_filter, set_view_sort.

Tests
- Sort by numeric field; stable sort behavior verified.
- Large-list performance sanity.

---

## Increment 5: Control Flow & Composition

Deliverables
- Control actions:
  - if(cond=$(…)) { … } else { … }
  - batch { … } (optional explicit; on already batches)
  - for_each(list=/path.filter(|x| …)) { … } with inner actions; bind `x` for var paths (x/field)

Tests
- for_each over Tasks increments priorities; guarded by if.
- Nested composition works and runs once per transaction.

---

## Increment 6: Time-based Scheduling and Catch-up

Deliverables
- schedule(after="2w" | at="2025-09-01T09:00" | every="1d@09:00", actions={ … }, id="…") inside an on block or document-level.
- Persistence fields per trigger instance: last_run, run_count.
- On document load: compute due runs between last_run and now; execute deterministically (by due time).
- Caps/limits (optional) and idempotent patterns via ensure_* actions.

Tests
- Simulated clock tests: after/every/at semantics; catch-up replays N invocations.
- Deterministic ordering across multiple triggers.

---

## Increment 7: Document/Structural Utilities

Deliverables
- ensure_document(path=/Docs/History/{yyyy-MM}, template=/HistoryTemplate, name="History {yyyy-MM}")
- clone(node=/path, to=/dest, shallow|deep)

Tests
- History-document creation flow; monthly roll-over.
- clone correctness; no ID/key collisions (keys honored).

---

## Increment 8: Observability, Safety, and Tooling

Deliverables
- Action log (in-memory) with levels; `log(message=…, level=info|warn|error)` action.
- Dry-run mode (dev flag) to print planned mutations without applying.
- Failure policy: stop-on-first vs continue; error surfacing to UI.

Tests
- Logs appear; dry-run prints sequences; failure policy honored.

---

## Acceptance Scenarios (mapped to your use cases)
1) Counter button → Increment 1–2
2) Add item button → Increment 3 (append)
3) Sort/filter button → Increment 4 (sort, optional view filter)
4) Complete task button (move by key) → Increment 3
5) Recurring task (reactivate after delay) → Increment 6 (schedule after)
6) Periodic task (daily/weekly) → Increment 6 (every)
7) Priority gain daily + auto-sort → Increments 5–6 (for_each + schedule + sort)
8) Auto task history → Increments 6–7 (schedule + ensure_document + move)

---

## Engine Contracts (concise)
- Context: Paths resolve relative to node owning the `on` block; absolute `/` and `..` supported; lambdas follow current evaluator rules.
- Transaction: Execute all actions in order; then 1x resolve + compute; then notify renderer.
- Mutability: Writes target raw values/structure (not `_computed_*` shadows).
- Determinism: Time triggers compute due windows from stored last_run; execution sorted by due timestamp.

---

## Testing Strategy
- Unit tests for each action and selector (key-based ops).
- Integration tests per acceptance scenario (simulate UI events and time).
- Property tests for sort stability and key uniqueness.
- Performance smoke: large lists (1k items) for sort/for_each.

---

## Rollout & Flags
- Feature flag `actions.enabled` (on by default after Increment 2).
- Scheduler flag `scheduler.enabled` (on by default after Increment 6).
- DEBUG logs in executor with compact, grep-friendly lines.

---

## Next Immediate Steps
- Implement Increment 0 + 1 skeletons:
  - Parser support for `on` and actions (accept, store in AST).
  - Executor with set/inc/toggle/clear/set_now.
  - Wire reevaluation post-transaction.
- Then Increment 2 wiring for button clicks.
