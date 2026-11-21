# Testing Guide

## The two suites

Overseer is tested from both sides of the Tauri IPC boundary:

- **Rust** (`src-tauri/`) — the language core: parser, resolver, formula evaluator,
  actions, and the serializer. Unit tests live beside the code they cover; the
  cross-cutting ones live in `src-tauri/tests/it/`, one module a file, built together as
  the one test binary `it` so the crate is linked once for all of them.
- **JavaScript** (`tests/`) — the renderer and app shell, run under jsdom with
  Vitest. These drive the real DOM the app builds and assert on what the user
  would see, with the backend stubbed.

`npm test` runs both, builds the desktop app, and prints a combined summary. The build is
there because the desktop binary is gated behind a feature `cargo test` does not turn on -
so the suite once stayed green while the app would not link.

```bash
npm test              # both suites
npm run test:js       # Vitest only
npm run test:rust     # cargo test only
npm run test:rust:watch
npm run test:verbose  # cargo test with stdout captured

cd src-tauri && cargo test <name>            # single Rust test
cd src-tauri && cargo test --test it <file>::  # one file of the integration tests
npx vitest run -c tests/vitest.config.mjs <file>   # single spec
```

VS Code: Command Palette → "Tasks: Run Task" → "Test: Run All Tests".

The modules of `it` run on threads of one process, and share what the crate keeps for the
whole process: the snapshots of every parse, the documents it holds worked out, the view state,
the folder of the current document, the overrides of its caches. Every test holds
`common::PROCESS`: alone, with `common::serial()`, when it sets or resets any of that or relies on
nobody else touching it, and beside the others, with `common::shared()`, otherwise - so a
`SourceRegistry::reset` never lands in the middle of another test's save. A new test takes one of
the two as its first line. A file that cannot share a process at all - it counts what the caches hold, sets
an environment variable the crate reads on every call, or changes the working directory - stays
a binary of its own beside `it/`, run with `cargo test --test <file>`.

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

### Your own documents

`real_documents_round_trip.rs` also round-trips documents that are not in the
repository - the ones a person actually keeps, which a parser or serializer change
can damage in ways no example shows. It reads every `.os` file at the top of each
folder named in `OVERSEER_REAL_DOCS`, separated as in `PATH` (`;` on Windows, `:`
elsewhere):

```bash
OVERSEER_REAL_DOCS="E:/docs;E:/docs/projects" cargo test --test it real_documents_round_trip:: -- --nocapture
```

Without it the test prints that no folders were named and passes; the examples and
the library are round-tripped either way.

## Recording known bugs

A reproducible bug that is not being fixed yet is worth a test marked
`#[ignore = "known bug: ..."]`, describing the observed behaviour. It stays out
of the red, documents the defect precisely, and turns into the regression test
by deleting one line. See `src-tauri/tests/it/comment_trivia.rs`.

On the JS side the equivalent is `it.skip` with a description of the expected
outcome — see `tests/weight_tracker_amount_calories_total_ui_bug.spec.js`.

## In the real page

The specs draw with jsdom against a fake backend, which has no layout and no real server. What a
person will see - a layout, a press and what it writes - is checked in the real page as well,
with `scripts/preview.mjs` (see "Seeing a document" in `DEVELOPMENT.md`).

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `master` and every pull request. It runs both suites
on Windows, macOS and Linux - `cargo test --features server --no-fail-fast` in `src-tauri`, then
the page's specs - and checks that the desktop app still compiles on macOS and Linux. Windows is
left out of that check because its build needs the WebView2 runtime, which isn't in the repository.
The checkout keeps line endings as committed (`core.autocrlf false`), because the serializer
tests compare bytes.

The tests need no system packages: the webview stack comes in only with the `desktop` feature.
On Linux, the desktop app itself needs, on Ubuntu:

```bash
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libxdo-dev libssl-dev build-essential
```

## Notes

- Serializer tests assert on exact bytes, including newline style. The golden
  corpus deliberately contains a CRLF fixture; do not normalize line endings
  when editing fixtures.
- Layout resolution alternates by depth: a node with no explicit `layout` takes
  the opposite of its parent, and a root node defaults to horizontal. Expected
  values in layout tests follow from that rule rather than from a fixed default.
