# Multiple Document Support — Plan and Progress

This document tracks the design and implementation of multiple-document support (imports/mounts, lazy loading, and index-backed cross-file aggregations).

## Goals
- Split complex documents into separate files.
- Import/embed a subtree from another file (lazy by default).
- Store monthly histories in separate files and avoid loading them unless requested.
- Cache summary stats across files and use them for queries and charts.
- Keep cross-file paths first-class and actions explicit.

## DSL Additions
- Cross-file paths: `../history/2025-08.os/Tasks` (absolute and relative).
- Mount node (lazy import):
  - `mount August (source=../history/2025-08.os/Tasks, lazy=true, placeholder="August Tasks", preload=summary)`
  - Optional: `<../history/2025-08.os/Tasks> August (lazy=true)`
- Index-backed file queries:
  - `files(pattern)` with reducers: `.count()`, `.sum(path, field)`, `.min/.max/.average`, `.range(path, field)`
  - Chart list-sources can read series from index: `files("../history/2025-*.os").map_monthly(Tasks.completed_count)`
- Cross-file actions (explicit):
  - `action append (target=../history/2025-09.os/Tasks) = { ... }`
- Mount state:
  - Computed params on the mount: `_mount_status`, `_summary_total`, `_summary_completed`, `_summary_date_range`, etc.

## Architecture
- DocumentManager (Rust): resolves absolute paths, manages open docs, and mediates index usage.
- IndexCache (Rust): sidecar JSON with `{ path, mtime, size, stats, version }`; invalidated by mtime changes.
- Resolver: supports file-segment path traversal; formulas prefer index where possible.
- Renderer: `mount` component shows placeholder + summary and a Load/Unload control.
- Canonical paths include `doc://` identity to keep actions stable across mounts.

## Phases and Tasks

### Phase 0 — Groundwork (Current)
- [x] Write plan and progress tracker (this file)
- [ ] Add DocumentManager module with stubs (open_document, get_index, query_files)
- [ ] Add IndexCache module with stubs (load/save, get/update entry)
- [ ] Wire modules in `main.rs` (no behavior change)
- [ ] Build & keep green

### Phase 1 — DSL & Parser
- [ ] Add `mount` node type with parameters (source,lazy,placeholder,preload)
- [ ] Normalize cross-file path segments in parser and path utils
- [ ] Add `files(pattern)` and reducers to formula evaluator (index-backed)
- [ ] Unit tests for parsing

### Phase 2 — Resolver & Evaluator
- [ ] Resolve file jumps in paths; prefer index for reads unless forced
- [ ] Mount nodes: set `_mount_status` and `_summary_*`; no children in lazy state
- [ ] Actions: add `load_mount`/`unload_mount`
- [ ] Tests for mount lifecycle and cross-file deref

### Phase 3 — Renderer
- [ ] Add visual mount component (placeholder, summary, buttons)
- [ ] Option: `openInTab=true` behavior

### Phase 4 — Index Content
- [ ] Built-in metrics: item_count, completed_count, created_min/max, due_min/max
- [ ] Optional `index` block to define custom metrics computed on save

### Phase 5 — Cross-File Charting
- [ ] Series builders using `files()` (monthly aggregates without loading docs)
- [ ] Chart tests against synthetic history files

### Phase 6 — Invalidation & Persistence
- [ ] mtime/size-based invalidation
- [ ] Update index on save (on the app’s save path)

### Phase 7 — Tests & Docs
- [ ] Unit & integration tests for monthly history scenario
- [ ] Docs and examples (`examples/`), including scheduler showcase

## Open Questions / Decisions
- Force-load controls in formulas: `forceLoad=true` param on reducers or path segments?
- Cycle detection strategy in mounts; UX for error state.
- Where to store cache: per-user app data vs project-local `.overseer/`.

## Progress Notes
- 2025-08-15: Draft plan created and accepted. Next: scaffold Phase 0 modules and keep build green.
