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

### Phase 0 — Groundwork (Completed)
- [x] Write plan and progress tracker (this file)
- [x] Add DocumentManager module with stubs (open_document, get_index, query_files)
- [x] Add IndexCache module with stubs (load/save, get/update entry)
- [x] Wire modules in `main.rs` (no behavior change)
- [x] Build & keep green

### Phase 1 — DSL & Parser
- [x] Add `mount` node type semantics (parameters: source, lazy, placeholder; validation in resolver)
- [ ] Normalize cross-file path segments in parser and path utils
- [x] Add `files(pattern)` stub to formula evaluator (enables reducers without I/O)
- [ ] Unit tests for parsing (parser currently accepts `mount` via generic node parsing)

### Phase 2 — Resolver & Evaluator
- [ ] Resolve file jumps in paths; prefer index for reads unless forced
- [x] Mount nodes: set `_mount_status` defaults (`unloaded`/`error`), preserve status across resolves
- [x] Actions: add `load_mount`/`unload_mount` with robust error handling (`_mount_error`)
- [x] Implicit default handling: mounts respond to `load`/`unload` without explicit `on` blocks
- [x] Tests for mount lifecycle (same-doc, external), and error cases (missing file, bad internal path)

### Phase 3 — Renderer
- [x] Add visual mount component (placeholder, status, Load/Unload controls)
- [x] Error UX: show `_mount_error` under status; disable Load while loading
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

## Implementation Notes (recent)
- Relative mount paths are resolved relative to the opened document directory (CWD set on file open).
- Mount source parsing supports `file.os/internal/path` and same-document internal paths.
- On mount load failure, `_mount_status = "error"` and `_mount_error` detail the reason.

## Progress Notes
- 2025-08-15: Draft plan created and accepted. Next: scaffold Phase 0 modules and keep build green.
- 2025-08-15 (later): Phase 0 completed. Phase 1 (mount semantics + files() stub) in place. Phase 2 load/unload implemented with tests. Renderer shows status/error and safe Load.
