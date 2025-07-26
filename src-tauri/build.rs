use std::path::Path;

fn main() {
    // Check if WebView2 runtime exists before building
    let webview2_path = Path::new("webview2/Microsoft.WebView2.FixedVersionRuntime.138.0.3351.95.x64");
    
    if !webview2_path.exists() {
        eprintln!("⚠️  WebView2 Fixed Version Runtime not found!");
        eprintln!("📁 Expected location: {}", webview2_path.display());
        eprintln!("📖 Please follow WEBVIEW2_SETUP.md instructions to set up WebView2 runtime");
        eprintln!("");
        eprintln!("Quick setup:");
        eprintln!("1. Download WebView2 Fixed Version Runtime (.cab file) from Microsoft");
        eprintln!("2. Extract it: expand \"path\\to\\runtime.cab\" \"webview2\" -F:*");
        eprintln!("3. Re-run the build");
        std::process::exit(1);
    }
    
    println!("✅ WebView2 runtime found");
    println!("🔧 Building Tauri application...");
    
    tauri_build::build()
}
