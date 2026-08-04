# Overseer

> A minimalistic personal data management framework using a custom domain-specific language

Overseer is a cross-platform desktop application that helps you manage personal statistics, projects, tasks, journals, and other data using a human-readable, hierarchical syntax. It prioritizes open standards, local data storage, and complete user control over your information.

## ✨ Features

- **Custom DSL**: Human-readable syntax for defining data structures, logic, and styling
- **Advanced Layout System**: Flexible horizontal/vertical layouts with spacing and margin controls
- **Rich Styling**: Colors, fonts, borders, and sizing with inheritance
- **Markdown Support**: Full markdown formatting with dual edit/view modes
- **Grid Layouts**: Fixed sizing and table-like structures
- **Local Storage**: All data stored in plain-text `.os` files
- **Cross-Platform**: Built with Tauri for Windows, macOS, and Linux
- **Live Updates**: Real-time file watching and auto-refresh

## 🚀 Quick Start

### Prerequisites

- [Node.js](https://nodejs.org/) (v16 or later)
- [Rust](https://rustup.rs/) (latest stable)
- [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (Windows only)

### Installation

1. **Clone the repository:**
   ```powershell
   git clone https://github.com/AxelStrem/Overseer.git
   cd Overseer
   ```

2. **Install dependencies:**
   ```powershell
   npm install
   ```

3. **Run in development mode:**
   ```powershell
   npm run tauri:dev
   ```

### Building for Release

```powershell
# Build with WebView2 runtime bundled (recommended)
npm run tauri:build:release

# Quick build (requires WebView2 installed separately)
npm run tauri:build
```

The built application will be in `src-tauri/target/release/bundle/`.

## 📝 Basic Tutorial

### Your First Document

Create a file called `my-tasks.os`:

```overseer
tab "My Tasks" {
    text welcome = "Welcome to Overseer!"
    
    div task_list (background-color=#F5F5F5, layout=vertical, spacing=10) {
        div task1 (border-style=solid 1px gray, background-color=white) {
            string title = "Learn Overseer syntax"
            text description = "Understand the basic node types and parameters"
            int priority = 5
            bool completed = false
        }
        
        div task2 (border-style=solid 1px gray, background-color=white) {
            string title = "Create a personal dashboard"
            text description (markdown=true) = "Build a **custom dashboard** with:
- Task tracking
- Statistics
- Project notes"
            int priority = 3
            bool completed = false
        }
    }
}
```

### Key Concepts

**Node Types:**
- `div` - Container for grouping and styling
- `string` - Single-line text
- `text` - Multi-line text (supports markdown)
- `int` - Numbers
- `bool` - True/false values
- `list` - Collections with templates
- `tab` - UI tabs for organization

**Layout System:**
```overseer
div container (layout=horizontal, spacing=15, margin=10) {
    // Children arranged horizontally with 15px gaps and 10px margin
}
```

**Styling:**
```overseer
div styled_box (
    background-color=lightblue,
    font-size=18px,
    font-color=darkblue,
    width=200px,
    height=100px,
    border-style=solid 2px navy,
    border-radius=8px
) {
    text content = "A styled container"
}
```

**Markdown Text:**
```overseer
text docs (markdown=true) = "# Project Notes
**Important:** This supports *formatting* and `code`!

- Feature list
- Progress tracking
- Documentation"
```

**Interactive Elements:**
```overseer
// Button with actions
button save_data "Save Progress" {
    action Set (target=../last_saved) = $(today())
    action Add (target=../save_count) = 1
}

// Checkbox with automatic actions
checkbox task_complete "Mark Done" {
    action Set (target=../completed) = true
    action Set (target=../completion_date) = $(today())
}

// Scheduled timer
timer daily_reminder (at="09:00", active=true) {
    action Set (target=../reminder_sent) = true
}
```

## 🔧 Development

### Running Tests

```powershell
# Run all tests
npm test

# Run tests in watch mode
npm run test:rust:watch

# Verbose test output
npm run test:verbose
```

### Debug Modes

```powershell
# Debug parser
npm run tauri:dev:debug-parser

# Debug resolver
npm run tauri:dev:debug-resolver

# Debug evaluator
npm run tauri:dev:debug-evaluator

# Debug all systems
npm run tauri:dev:debug
```

### Project Structure

```
overseer/
├── src/            # Frontend: renderer, app shell, styles
├── src-tauri/      # Backend: the language core and file I/O
├── tests/          # Frontend specs (Vitest + jsdom)
├── examples/       # Example and personal .os documents
└── feature-plans/  # Design docs for individual subsystems
```

A document flows through the backend in one direction:

**parse** (`parser.rs` — text to AST, retaining each node's source span and
surrounding trivia) → **resolve** (`resolver.rs` — parameter inheritance, layout,
template instantiation) → **evaluate** (`formula_evaluator.rs` with
`dependency_tracker.rs` deciding what actually needs recomputing) → **serialize**
(`file_ops.rs`).

The serializer is a *patcher*, not a generator: it replays each node's original
source text unless that node's fingerprint changed, so an edit to one field
rewrites one line and leaves the rest of the file — comments, spacing, parameter
order — byte-identical. Keeping that property is the reason for most of the
complexity in the parse and serialize stages.

## 📚 Documentation

- [Syntax Specification](overseer_syntax_specification.md) - Complete language reference
- [Technical Architecture](technical_architecture.md) - System design details
- [Development Plan](DEVELOPMENT_PLAN.os) - Current roadmap and progress
- [Product Description](product_description.md) - Vision and use cases

## 🎯 Current Status

Overseer is usable for real personal tracking — the documents under `examples/`
are live data, not demos. The DSL, styling, layout, formulas, actions and charts
all work; the parser, resolver and serializer are the mature parts of the system.

Progress is tracked in the project's own format rather than restated here:

- [DEVELOPMENT_PLAN.os](DEVELOPMENT_PLAN.os) — phases and their tasks, each with
  `complete` and `tested` flags. This is the authoritative roadmap.
- [KNOWN_BUGS.os](KNOWN_BUGS.os) — open bugs and requested features. Entries with
  `fixed = false` are the live ones.
- [feature-plans/](feature-plans/) — design and progress notes per subsystem.

Broadly: **multi-file support** (imports, mounts, cross-file queries) is designed
in `feature-plans/MULTI_DOCUMENT_SUPPORT_PLAN.md` but not yet implemented, and
advanced UI components and performance work are still open.

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature-name`
3. Make your changes and test thoroughly
4. Commit with clear messages: `git commit -m "Add feature description"`
5. Push and create a pull request

## 🛟 Support

- Check the [examples/](examples/) directory for sample files
- Review [KNOWN_BUGS.os](KNOWN_BUGS.os) for current issues
- See [DEVELOPMENT_SETUP.md](DEVELOPMENT_SETUP.md) for detailed setup instructions

---

**Built with ❤️ using [Tauri](https://tauri.app/) • A modern approach to personal data management**
