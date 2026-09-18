# Architecture

How a `.os` file becomes a screen, and why the parts are split where they are.

## The shape of it

One library, two front ends. `src-tauri/src/lib.rs` holds the language — parsing, resolving,
evaluating, serializing — and knows nothing about either. On top of it sit the desktop app
(`main.rs`, Tauri commands, a webview running `src/`) and the HTTP server
(`bin/server.rs`, axum). Both are cargo features and neither is on by default, so `cargo build`
gives the library alone, which is what the tests use.

Everything either front end does goes through `app_api.rs`. That split is not cosmetic: before
it, the commands lived in the binary where no test could reach them, and a bug that mangled
documents lived in the gap between what the tests assumed and what the commands did.

## The pipeline

```
text ──parser──► nodes ──resolver──► resolved nodes ──renderer──► DOM
                              │                  ▲
                              │                  │
                         formula_evaluator   actions (a button was pressed)
                              │
                         dependencies (what each value came from)

resolved nodes ──file_ops + source_registry──► text again, byte for byte
```

**Resolving** is repeated passes rather than one: templates instantiate entries, entries
inherit, formulas read values that other formulas produce. A pass that changes nothing ends it.

**Serializing** is not "print the tree". `source_registry.rs` remembers where every node came
from in the original text, so comments, blank lines and authored spacing survive a save. Two
rules follow from how it writes: parameters go on one line, and an entry's fields go in the
order its template declares them. A document written any other way reformats itself the first
time anything writes to it.

## The modules

| | |
| --- | --- |
| `parser.rs` | text to nodes |
| `types.rs` | `OverseerNode`, `OverseerValue` — what everything else passes around |
| `resolver.rs` | templates, inheritance, multipass, and which slice of a list is in view |
| `formula_evaluator.rs` | `$(...)`: arithmetic, comparisons, and the `filter`/`map`/`sum` chains |
| `actions.rs` | what a button does: append, remove, set |
| `addressing.rs` | stable addresses, so a thing read can be written afterwards |
| `dependencies.rs` | what each computed value was worked out from |
| `document_cache.rs` | what has already been worked out, kept while it is worth keeping |
| `delta.rs` | what changed between two resolves, so the UI redraws two nodes not the document |
| `file_ops.rs` | reading, writing, and the serializer |
| `source_registry.rs` | where each node came from in the text |
| `docmgr/` | which document is open, so a mount resolves against its own directory |
| `app_api.rs` | the entry points both front ends call |
| `server.rs`, `bin/server.rs` | the same, over HTTP |

## What makes it fast enough

A real document is tens of thousands of nodes with formulas over lists, and the naive version
of all this took over a minute to open. Four things brought that down, and each is worth
knowing about because each has a cost:

- **A dependency graph.** Recording what each value read costs about a quarter of a first open
  and makes the second one almost free. On by default; `OVERSEER_DEPENDENCY_GRAPH=0` turns it
  off, for measuring against it or when a graph is suspected of holding a stale answer.
- **A document cache**, byte-budgeted and least-recently-used, holding text, nodes and graph
  together. `OVERSEER_CACHE_MB` overrides the budget. A document resolved with part of a list
  out of view is never cached, because a write needs the whole thing.
- **Windowed lists.** `window=3` on a list resolves three entries instead of forty-three, and
  the window is applied before templates are instantiated rather than after, so the work is
  never done.
- **An equality index** for `filter`, built per pass and keyed by the field compared. Strings
  only: `compare_values` falls back to text, so `5 == "5"` while `5.0 != "5.0"`, and an index
  over a comparison that is not transitive would answer differently from the scan it replaces.

## The invariant

Loading a document and saving it without touching anything gives back the same bytes, however
many times it is repeated. Several real documents are checked that way in
`src-tauri/tests/app_load_save_cycle.rs`, and the serializer's own fixtures in
`tests/fixtures/serializer/` are checked against golden files.

It is the invariant the app promises and the one most easily broken by accident, because
nothing about a wrong answer here is visible until something writes — and then it is visible
as a diff touching every entry in the document.
