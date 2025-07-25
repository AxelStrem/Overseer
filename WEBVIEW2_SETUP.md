# WebView2 Fixed Version Setup

This project uses a fixed version of Microsoft WebView2 runtime to ensure consistent behavior across all Windows systems.

## For Development/Building

To set up the WebView2 fixed version runtime:

1. Download the WebView2 Fixed Version Runtime from Microsoft:
   - Go to https://developer.microsoft.com/en-us/microsoft-edge/webview2/
   - Download the "Fixed Version" distributable (not the installer)
   - Choose the x64 version (usually named like `Microsoft.WebView2.FixedVersionRuntime.XXX.X.XXXX.XX.x64.cab`)

2. Extract the runtime:
   ```powershell
   # From the project root directory
   cd src-tauri
   expand "path\to\Microsoft.WebView2.FixedVersionRuntime.XXX.X.XXXX.XX.x64.cab" "webview2" -F:*
   ```

3. Build the application:
   ```powershell
   # From project root
   npm run tauri build
   ```

## How It Works

The application is configured to use the bundled WebView2 runtime through:

- **Environment Variables**: Set in `src-tauri/src/main.rs` to point to the local runtime
- **Resource Bundling**: WebView2 files are included in the release bundle via `tauri.conf.json`
- **Fixed Version Path**: Runtime is expected at `./webview2/Microsoft.WebView2.FixedVersionRuntime.XXX.X.XXXX.XX.x64/`

## Benefits

- ✅ No dependency on system-installed WebView2 runtime
- ✅ Consistent behavior across all Windows systems
- ✅ Eliminates WebView2 `0x80070002` errors
- ✅ Self-contained distribution

## File Structure

```
src-tauri/
├── webview2/
│   └── Microsoft.WebView2.FixedVersionRuntime.XXX.X.XXXX.XX.x64/
│       ├── msedgewebview2.exe
│       ├── msedge.dll
│       └── ... (other WebView2 runtime files)
└── target/release/
    └── (WebView2 files copied here during build)
```

**Note**: The `webview2/` directory is gitignored due to its large size (~300MB). Each developer needs to set it up locally using the steps above.
