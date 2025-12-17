---
applyTo: '**'
---
Use proper PowerShell syntax for terminal commands; Don't use && 

Git commands (especially "git commit") complete immediately and silently in PowerShell. DO NOT wait for output or use get_terminal_output after git commands - proceed immediately to the next action. If you find yourself waiting after a git command, you are likely stuck and should move on.

Always consult product_description.md and technical_architecture.md for project-specific guidelines and architecture details.

Consult overseer_syntax_specification.md for Overseer language syntax; Keep the syntax in mind when developing app features.

## Tauri Application Launch Commands:
- **Development Mode**: Use `npm run tauri:dev` (NOT `cargo run`) - this starts both Vite dev server and Tauri backend
- **Release Build**: Use `npm run tauri:build:release` - builds optimized release version with pre-build checks
- **Basic Release Build**: Use `npm run tauri:build` - builds without pre-checks (faster but may fail if WebView2 missing)
- **Never use `cargo run` directly** - it only starts backend without frontend, causing "can't reach this page" errors

## Release Build Process:
The enhanced release build (`npm run tauri:build:release`) automatically:
1. ✅ Checks for WebView2 Fixed Version Runtime presence
2. ✅ Runs frontend build (`vite build`)
3. ✅ Bundles WebView2 runtime files with the application
4. ✅ Creates self-contained executable in `src-tauri/target/release/bundle/`

If WebView2 runtime is missing, follow WEBVIEW2_SETUP.md instructions.