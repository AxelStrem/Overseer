use std::path::Path;

fn main() {
    // Only the desktop app needs any of this. The server binary is built without the `desktop`
    // feature - in a Linux container, among other places - where there is no webview to find
    // and no Tauri application to configure. Exiting with an error there would fail a build
    // that has nothing to do with any of it.
    if std::env::var("CARGO_FEATURE_DESKTOP").is_err() {
        return;
    }

    // The runtime is a Windows artefact; nothing looks for it elsewhere.
    if cfg!(target_os = "windows") {
        let webview2_path =
            Path::new("webview2/Microsoft.WebView2.FixedVersionRuntime.138.0.3351.95.x64");
        if !webview2_path.exists() {
            eprintln!("⚠️  WebView2 Fixed Version Runtime not found!");
            eprintln!("📁 Expected location: {}", webview2_path.display());
            eprintln!("📖 Please follow WEBVIEW2_SETUP.md instructions to set up WebView2 runtime");
            eprintln!();
            eprintln!("Quick setup:");
            eprintln!("1. Download WebView2 Fixed Version Runtime (.cab file) from Microsoft");
            eprintln!("2. Extract it: expand \"path\to\runtime.cab\" \"webview2\" -F:*");
            eprintln!("3. Re-run the build");
            std::process::exit(1);
        }
        println!("✅ WebView2 runtime found");
    }

    tauri_build::build()
}
