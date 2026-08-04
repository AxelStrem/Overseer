# Testing Guide

## The two suites

Overseer is tested from both sides of the Tauri IPC boundary:

- **Rust** (`src-tauri/`) — the language core: parser, resolver, formula evaluator,
  actions, and the serializer. Unit tests live beside the code they cover; the
  cross-cutting ones live in `src-tauri/tests/`.
- **JavaScript** (`tests/`) — the renderer and app shell, run under jsdom with
  Vitest. These drive the real DOM the app builds and assert on what the user
  would see, with the backend stubbed.

`npm test` runs both and prints a combined summary.

```bash
npm test              # both suites
npm run test:js       # Vitest only
npm run test:rust     # cargo test only
npm run test:rust:watch
npm run test:verbose  # cargo test with stdout captured

cd src-tauri && cargo test <name>            # single Rust test
npx vitest run -c tests/vitest.config.mjs <file>   # single spec
```

VS Code: Command Palette → "Tasks: Run Task" → "Test: Run All Tests".

## How the JS specs talk to the backend

`@tauri-apps/api` reaches the backend through `window.__TAURI_INTERNALS__`, which
does not exist under jsdom. Two mechanisms cover that:

- Specs that assert on backend calls install their own mock:
  `vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))`, then drive
  `invoke.mockImplementation(...)` to play the role of the Rust side.
- Specs that only exercise UI behaviour rely on the shared bridge in
  `tests/helpers/tauri-bridge.js`, installed globally by
  `tests/setup-tauri-internals.js`, which makes an unmocked `invoke` resolve
  quietly instead of throwing.

`bootstrapMinimalDom()` swaps in a fresh JSDOM instance, so it re-installs the
bridge itself. If you build a window some other way, install the bridge on it.

Import `invoke` from `@tauri-apps/api/core`. The Tauri v1 path
(`@tauri-apps/api/tauri`) no longer resolves, and a spec that mocks it silently
fails to intercept anything.

## Fixtures

Serializer and round-trip tests read `.os` documents from two places, and the
distinction matters:

- **`examples/`** holds live user data that changes whenever the app is used.
  Only assertions that hold for *any* content may read it — for example
  `exercise_round_trip_preserves_text`, which asserts that an untouched
  parse → resolve → serialize returns the file byte-for-byte, whatever it contains.
- **`src-tauri/tests/fixtures/`** holds frozen documents. Anything that targets a
  specific node, line, or value belongs here, because an assertion anchored to
  live data breaks the next time the user edits their document rather than when
  the code regresses.

`src-tauri/tests/fixtures/serializer/` is auto-discovered by
`serializer_golden.rs`; every `.os` file dropped there joins the golden corpus.
Fixtures for a single test go directly in `fixtures/`.

## Recording known bugs

A reproducible bug that is not being fixed yet is worth a test marked
`#[ignore = "known bug: ..."]`, describing the observed behaviour. It stays out
of the red, documents the defect precisely, and turns into the regression test
by deleting one line. See `src-tauri/tests/comment_trivia.rs`.

On the JS side the equivalent is `it.skip` with a description of the expected
outcome — see `tests/weight_tracker_amount_calories_total_ui_bug.spec.js`.

## Notes

- Serializer tests assert on exact bytes, including newline style. The golden
  corpus deliberately contains a CRLF fixture; do not normalize line endings
  when editing fixtures.
- Layout resolution alternates by depth: a node with no explicit `layout` takes
  the opposite of its parent, and a root node defaults to horizontal. Expected
  values in layout tests follow from that rule rather than from a fixed default.
