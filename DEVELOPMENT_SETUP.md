# Overseer - Development Setup Guide

## Prerequisites Installation

### 1. Install Rust

**Windows:**
- Download and run the installer from: https://rustup.rs/
- This will install `rustc`, `cargo`, and the Rust toolchain
- After installation, restart your terminal/command prompt

**Alternative via Chocolatey (if you have it):**
```powershell
choco install rust
```

**Alternative via Scoop (if you have it):**
```powershell
scoop install rust
```

### 2. Install Node.js

**Windows:**
- Download the LTS version from: https://nodejs.org/
- Run the installer (includes npm automatically)
- Verify installation: `node --version` and `npm --version`

**Alternative via Chocolatey:**
```powershell
choco install nodejs
```

**Alternative via Scoop:**
```powershell
scoop install nodejs
```

### 3. Install Tauri CLI

After installing Rust and Node.js:

```bash
# Install Tauri CLI via Cargo
cargo install tauri-cli

# Or install via npm
npm install -g @tauri-apps/cli
```

### 4. Install WebView2 (Windows only)

Tauri requires WebView2 for the web frontend:
- Download from: https://developer.microsoft.com/en-us/microsoft-edge/webview2/
- Most modern Windows 10/11 systems already have this

## Project Setup

### 1. Clone/Navigate to Project
```bash
cd "e:\Source\Repos\Overseer"
```

### 2. Install Dependencies

**Frontend dependencies:**
```bash
npm install
```

**Rust dependencies (automatically handled):**
```bash
cargo check
```

### 3. Development Commands

**Start development server:**
```bash
npm run tauri dev
# OR
cargo tauri dev
```

**Build for production:**
```bash
npm run tauri build
# OR
cargo tauri build
```

**Run tests:**
```bash
# Rust tests
cargo test

# Frontend tests (if added)
npm test
```

## Development Workflow

### 1. File Structure Overview
```
Overseer/
├── src-tauri/           # Rust backend
│   ├── src/
│   │   ├── main.rs      # Tauri entry point
│   │   ├── parser.rs    # DSL parser
│   │   ├── types.rs     # Data structures
│   │   ├── evaluator.rs # Formula evaluation
│   │   └── file_ops.rs  # File operations
│   ├── Cargo.toml       # Rust dependencies
│   └── tauri.conf.json  # Tauri configuration
├── src/                 # Frontend
│   ├── main.js          # Application logic
│   ├── renderer.js      # UI rendering
│   ├── file-manager.js  # File management
│   └── styles.css       # Styling
├── index.html           # HTML entry point
├── package.json         # Node.js dependencies
└── example.os           # Sample Overseer document
```

### 2. Making Changes

**Backend (Rust) changes:**
- Edit files in `src-tauri/src/`
- Tauri will automatically rebuild and restart

**Frontend changes:**
- Edit files in `src/` or `index.html`
- Changes will hot-reload automatically

### 3. Testing the Application

1. **Start development server:**
   ```bash
   npm run tauri dev
   ```

2. **Open a test file:**
   - Click "Open File" in the application
   - Navigate to `example.os` in the project root
   - The file should parse and display the dashboard

3. **Test features:**
   - Tab navigation between "personal_dashboard" and "projects"
   - Double-click fields to edit them
   - Check that formulas display calculated values

## Troubleshooting

### Common Issues

**"npm/cargo not found":**
- Ensure Node.js and Rust are properly installed
- Restart your terminal after installation
- Check PATH environment variables

**Missing icon files error:**
- Download a simple ICO file and place it at `src-tauri/icons/icon.ico`
- Or download from: https://icons8.com/icons/set/app (save as .ico format)
- You can also use any 32x32 pixel ICO file for development

**"WebView2 not found" (Windows):**
- Install Microsoft Edge WebView2 Runtime
- Available from Microsoft's website

**"Permission denied" errors:**
- Run terminal as administrator (Windows)
- Check file permissions in the project directory

**Build fails with dependency errors:**
- Clear npm cache: `npm cache clean --force`
- Delete `node_modules` and run `npm install` again
- Update Rust: `rustup update`

### Performance Tips

**Development:**
- Use `cargo tauri dev` for faster Rust compilation
- Enable incremental compilation in `Cargo.toml`
- Use `--release` flag only for production builds

**Production:**
- Run `cargo tauri build --release` for optimized builds
- Enable all compiler optimizations
- Consider using `wee_alloc` for smaller binary size

## IDE Setup

### VS Code (Recommended)

**Essential Extensions:**
- `rust-analyzer` - Rust language support
- `Tauri` - Tauri-specific support
- `ES6 String HTML` - Template literal highlighting

**Settings:**
```json
{
    "rust-analyzer.cargo.features": "all",
    "rust-analyzer.checkOnSave.command": "clippy"
}
```

### Other IDEs

**IntelliJ IDEA:**
- Install Rust plugin
- Configure Cargo integration

**Vim/Neovim:**
- Use `rust.vim` and `coc-rust-analyzer`

## Next Steps

### Phase 1: Complete Basic Functionality
1. ✅ Project setup and structure
2. 🔄 Complete parser for all syntax features
3. ⏳ Implement formula evaluation
4. ⏳ Add CRUD operations for all field types
5. ⏳ Polish UI and interactions

### Phase 2: Advanced Features
1. Chart integration with Chart.js
2. Action/trigger system
3. Cross-file references
4. Export/import capabilities
5. Mobile app development

### Getting Help

**Documentation:**
- Tauri: https://tauri.app/
- Rust: https://doc.rust-lang.org/
- nom parser: https://docs.rs/nom/

**Community:**
- Tauri Discord: https://discord.com/invite/SpmNs4S
- Rust Users Forum: https://users.rust-lang.org/

**Project Issues:**
- Check the parser implementation in `src-tauri/src/parser.rs`
- Test with simple .os files first
- Use `console.log` for frontend debugging
- Use `println!` for Rust backend debugging
