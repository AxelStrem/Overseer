# Overseer

Personal data kept in plain text, in files you own, described by a small language.

A document is a `.os` file. It says what the data is, how it is laid out, what is computed from
it and what the buttons do — all in one place, all human-readable, all in your working copy.
There is no database and no service in the middle.

```
tab tracker (label="Today", mutable=true) {
    list History (entry=<Day>, key="day", window=3, sort_by=$(|x| 0 - millis_since_epoch(x/day))) { }

    div Day (layout="horizontal", spacing=6) {
        timestamp day (format="date") = "2026-01-01T00:00:00Z"
        float calories (format="trim") = 0
        float share (label="of goal") = $(calories / 2000 * 100)
    }
}
```

## Two ways to run it

**As a desktop app.** Tauri wraps the renderer and the resolver in one window.

```bash
npm install
npm run tauri:dev
```

**As a server.** The same resolver over HTTP, so something other than a person can read and
write the documents — which is what the companion bot does.

```bash
npm run server                                   # or, directly:
cargo run --features server --bin overseer-server -- --root <documents>
```

It binds to loopback unless told otherwise. Reachable from outside, it refuses to start without
`OVERSEER_TOKEN` — a document set is someone's diary, and an open port is an open diary.

## The language

Nodes are typed and nest. Containers are `tab`, `div` and `list`; values are `string`, `int`,
`float`, `bool` and `timestamp`; the rest draw or do something — `text`, `button`, `chart`,
`plot`, `tags`, `filter`, `timer`, `checkbox`, `mount`.

Four ideas carry most of the weight:

- **Templates.** A `div` declared with a name becomes the shape of a list's entries, and each
  entry says only what differs from it.
- **Formulas.** `$(...)` computes a value from others, addressed by path. `..` walks up until it
  finds the name, so moving a field between wrappers does not break what reads it.
- **Actions.** A `button` holds `on click { append ... }` or `remove`, so a document changes
  itself without anything outside it knowing its shape.
- **Mounts.** `mount` pulls another document in, so a catalogue can be shared between documents
  that each keep their own history.

[overseer_syntax_specification.md](overseer_syntax_specification.md) is the reference.

## Where things are

| | |
| --- | --- |
| `src/` | the renderer: DOM, charts, styling |
| `src-tauri/src/` | the language: parser, resolver, formula evaluator, actions, serializer |
| `src-tauri/src/bin/server.rs` | the HTTP server |
| `examples/` | real documents, several of which the tests resolve |
| `tests/`, `src-tauri/tests/` | the two suites |

## Testing

```bash
npm test          # both suites, and a desktop build that must link
npm run test:js   # the renderer, under jsdom
npm run test:rust # the language core
```

See [TESTING.md](TESTING.md). The one rule worth knowing up front: a save must give back the
bytes it was given, and several real documents are checked that way, because a document that
reformats itself on the first button press is a diff nobody can review.

## Reading further

- [overseer_syntax_specification.md](overseer_syntax_specification.md) — the DSL, node by node
- [technical_architecture.md](technical_architecture.md) — how a document becomes a screen
- [DEVELOPMENT.md](DEVELOPMENT.md) — toolchain, debug builds, WebView2
- [product_description.md](product_description.md) — what this is for, written before it existed
