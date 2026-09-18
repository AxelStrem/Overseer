# Development

## What you need

- **Rust** — [rustup.rs](https://rustup.rs/). On Windows this also wants the MSVC build tools,
  which the rustup installer offers to fetch.
- **Node 18 or later** — [nodejs.org](https://nodejs.org/).

Nothing else is installed globally. The Tauri CLI comes from `package.json`, and cargo fetches
the Rust dependencies on the first build.

```bash
npm install
```

## Running it

```bash
npm run tauri:dev            # the desktop app
npm run tauri:dev:release    # the same, optimised - worth it on a large document
npm run server               # the HTTP server instead, on 127.0.0.1:4747
npm run dev                  # the renderer alone under Vite, with no backend
```

The two halves are separate cargo features, and neither is on by default: `desktop` pulls in
Tauri, `server` pulls in axum. `cargo build` with neither builds the library and is what the
tests use.

## Building for release

```bash
npm run tauri:build          # a build you can run
npm run tauri:build:release  # a build to hand to someone, WebView2 bundled
```

## Debug logging

Three cargo features, each printing what one stage of the pipeline decided. They are compile-time,
so an ordinary build carries none of it.

```bash
npm run tauri:dev:debug             # all three
npm run tauri:dev:debug-parser      # what the parser read
npm run tauri:dev:debug-resolver    # what templates and inheritance produced
npm run tauri:dev:debug-evaluator   # every formula and what it worked out
```

Expect a lot of output. The resolver's is per node per pass, which on a real document is
thousands of lines — useful when a value is wrong, unreadable otherwise. Redirect it to a file.

## WebView2 on Windows

Release builds bundle a fixed version of the WebView2 runtime rather than using whatever is
installed, so the app behaves the same on every machine and cannot fail with `0x80070002`.

It is not in the repository — it is about 300MB — so set it up once:

1. Download the **Fixed Version** runtime (x64) from
   [the WebView2 page](https://developer.microsoft.com/en-us/microsoft-edge/webview2/). It
   arrives as a `.cab`.
2. Expand it into `src-tauri/webview2/`:
   ```powershell
   cd src-tauri
   expand "path\to\Microsoft.WebView2.FixedVersionRuntime.<version>.x64.cab" "webview2" -F:*
   ```

`main.rs` points `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` at it, `tauri.conf.json` bundles it, and
`npm run tauri:build:release` checks it is there before starting — the version it expects is in
the `prebuild:check` script. Development builds do not need any of this.

## Tests

`npm test`. See [TESTING.md](TESTING.md).
