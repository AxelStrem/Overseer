# Overseer - Technical Architecture Specification

## Project Overview

Overseer is a cross-platform personal data management framework featuring a custom Domain-Specific Language (DSL) for creating structured, interactive documents. The system enables users to define data schemas, perform calculations, and create dynamic interfaces using a hierarchical, human-readable syntax.

## High-Level Architecture

### Technology Stack

#### Core Framework
- **Tauri**: Cross-platform desktop application framework
  - Rust backend for native performance and security
  - Web frontend using HTML/CSS/JavaScript
  - Native APIs for file system, OS integration
  - Single binary distribution

#### Backend (Rust)
- **nom**: Parser combinator library for DSL parsing
- **tokio**: Async runtime for non-blocking I/O operations
- **serde**: Serialization/deserialization framework
- **chrono**: Date and time manipulation
- **notify**: File system watching for live updates

#### Frontend (JavaScript)
- **Vite**: Modern build tool and development server
- **Chart.js**: Data visualization library
- **Native DOM**: Direct HTML/CSS manipulation (no framework)

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    Frontend (Web)                       │
├─────────────────────────────────────────────────────────┤
│  UI Layer: HTML/CSS/JavaScript                         │
│  - Document Renderer                                   │
│  - Interactive Editor                                  │
│  - Chart Visualizations                               │
│  - File Management Interface                          │
└─────────────────────┬───────────────────────────────────┘
                      │ Tauri IPC
┌─────────────────────▼───────────────────────────────────┐
│                  Backend (Rust)                        │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │   Parser    │  │  Evaluator  │  │ File Ops    │     │
│  │   (nom)     │  │  (formulas) │  │ (async I/O) │     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
│                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │    Types    │  │   Actions   │  │   Storage   │     │
│  │    (AST)    │  │ (triggers)  │  │    (.os)    │     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                File System                             │
├─────────────────────────────────────────────────────────┤
│  .os files (Overseer documents)                       │
│  Cross-file references and imports                    │
│  Version control friendly (plain text)                │
└─────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Parser Module (`parser.rs`)
**Purpose**: Convert Overseer DSL syntax into Abstract Syntax Tree (AST)

**Key Features**:
- nom-based combinator parsing
- Support for all node types (tab, div, list, etc.)
- Formula parsing with $() syntax
- Parameter parsing with parentheses syntax
- Hierarchical structure handling
- Error recovery and reporting

**Input**: Raw .os file content
**Output**: Structured AST representation

### 2. Type System (`types.rs`)
**Purpose**: Define data structures for AST and runtime values

**Core Types**:
```rust
pub enum OverseerNode {
    Container { name: String, node_type: String, parameters: HashMap<String, String>, children: Vec<OverseerNode> },
    Field { name: String, node_type: String, parameters: HashMap<String, String>, value: Option<OverseerValue> }
}

pub enum OverseerValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Date(String),
    Formula(String),
    Array(Vec<OverseerValue>)
}
```

### 3. Resolver Module (`resolver.rs`)
**Purpose**: Process AST to resolve parameters, inheritance, and layout logic

**Key Features**:
- Parameter inheritance system (children inherit parent styling)
- Layout resolution (horizontal/vertical with alternation)
- Formula preparation and context building
- Template resolution and instantiation
- Reference path validation

### 4. Formula Evaluator (`formula_evaluator.rs`)
**Purpose**: Process and evaluate $() formulas within documents

**Features**:
- Expression parsing and evaluation
- Built-in functions (today(), sum(), avg(), etc.)
- Path resolution for cross-node references
- List operations and aggregations
- Type coercion and validation

### 5. File Operations (`file_ops.rs`)
**Purpose**: Handle all file system interactions asynchronously

**Operations**:
- Read/write .os files with comment preservation
- Canonical serialization and merge operations
- Asynchronous I/O using tokio
- Error handling and recovery
- File format validation

### 6. Renderer (`renderer.js`)
**Purpose**: Generate interactive UI from parsed AST with advanced styling

**Capabilities**:
- Dynamic HTML generation from node hierarchy
- Advanced CSS styling (colors, fonts, borders, sizing)
- Layout system (horizontal/vertical with spacing/margins)
- Markdown rendering with dual edit/view modes
- Interactive editing (double-click to edit)
- Tab management and navigation
- Parameter inheritance visualization

## Data Flow

### Document Loading Process
1. User selects .os file through UI
2. Frontend calls `load_overseer_file` Tauri command
3. Backend reads file asynchronously
4. Parser converts content to AST
5. Resolver processes inheritance, layout, and parameters
6. AST returned to frontend via IPC
7. Renderer generates HTML with advanced styling
8. Interactive UI displayed to user

### Formula Evaluation Workflow
1. Parser identifies $() formulas during parsing
2. Resolver prepares formula context and dependencies
3. Evaluator resolves references and executes calculations
4. Built-in functions executed with current context
5. Results cached for performance
6. UI updated with calculated values
7. Re-evaluation triggered on data changes

### File Saving Process
1. User modifies data through UI
2. Frontend updates local AST representation
3. Save command calls backend with regenerated content
4. Backend preserves comments using merge algorithm
5. Canonical serialization ensures consistent formatting
6. File written to disk asynchronously
7. Status indicators updated in UI

## DSL Syntax Specification

### Node Types
- **tab**: Top-level navigation containers
- **div**: Generic containers with layout options
- **list**: Collections with iteration support. Uses an `entry` parameter to define the type or template for its items (e.g., `entry=string` or `entry=<../Template>`).
- **string**: Text input fields
- **text**: Multi-line text with markdown support
- **int/float**: Numeric input fields
- **date**: Date picker components
- **bool**: Checkbox/toggle controls
- **button**: Interactive action triggers
- **chart**: Data visualization components

### Parameter System
Parameters modify node behavior and appearance with inheritance:
```overseer
// Styling parameters with inheritance
div container (background-color=lightblue, font-size=16px) {
    text child1 = "Inherits lightblue background and 16px font"
    text child2 (font-color=red) = "Inherits background/size, overrides color"
}

// Layout parameters
div layout_demo (layout=horizontal, spacing=10, margin=5) {
    // Children arranged horizontally with 10px gaps and 5px margin
}

// Advanced styling
div styled_box (
    width=200px,
    height=100px,
    border-style=solid 2px navy,
    border-radius=8px,
    overflow=hidden
) {
    // Fixed size box with styled border and clipped overflow
}
```

### Layout System
Advanced layout capabilities:
- **Direction**: `horizontal`, `vertical`, `inherit`, `opposite`
- **Spacing**: Gap control between child elements
- **Margins**: Individual side control (`margin-top`, `margin-bottom`, etc.)
- **Inheritance**: Children automatically alternate or inherit parent layout
- **Grid Support**: Fixed sizing with CSS units (px, %, em, vw, vh, fit-content, auto)

### Styling System
Comprehensive visual control:
- **Colors**: Hex (#FF0000), named (red), RGB triplets (rgb(0.2, 0.8, 0.5))
- **Typography**: Font size with multiple units, font color with inheritance
- **Borders**: Style (solid/dashed/dotted), selective sides, corner radius
- **Sizing**: Width/height with CSS unit support
- **Overflow**: Content clipping control (hidden, scroll, auto)

### Markdown Support
Rich text formatting:
```overseer
text content (markdown=true) = "# Heading
**Bold** and *italic* text
- Lists
- `Code`
- [Links](url)
> Blockquotes"
```

### Formula Language
Formulas enable dynamic calculations:
```
field_name = $(today() + 30)
total = $(sum(expenses.amount))
average = $(avg(scores[]))
```

## Development Phases

### Phase 1: Foundation ✅ COMPLETE
- ✅ Project structure setup with Tauri
- ✅ Complete parser implementation (nom-based)
- ✅ File I/O operations with comment preservation
- ✅ Advanced UI rendering with styling system
- ✅ Layout system (horizontal/vertical/spacing/margins)

### Phase 2: Advanced Display ✅ COMPLETE
- ✅ Complete styling system (colors, fonts, borders, sizing)
- ✅ Grid layouts with fixed sizing and overflow control
- ✅ Markdown text formatting with dual edit/view modes
- ✅ Parameter inheritance throughout node hierarchy
- ✅ Advanced border controls (selective sides, radius)

### Phase 3: Interactive Editing 🔄 IN PROGRESS
- 🔄 Enhanced field editing with validation
- 🔄 List CRUD operations (add/remove/reorder)
- ⏳ Undo/redo functionality
- ⏳ Drag-and-drop reordering

### Phase 4: Actions & Triggers ✅ COMPLETE
- ✅ Action system parsing and execution
- ✅ Interactive elements (buttons, checkboxes, timers)
- ✅ Conditional triggers and automation
- ✅ Timer-based scheduled actions
- ✅ Safe action execution with error handling

### Phase 5: Chart Visualization ✅ COMPLETE
- ✅ Chart.js integration with proper scaling
- ✅ Chart/plot DSL syntax support
- ✅ Data binding from computed series
- ✅ Interactive features (hover, tooltips, legend)
- ✅ Responsive design and resize handling

### Phase 6: Formula System 🔄 IN PROGRESS
- 🔄 Complete formula evaluator with $() syntax
- ⏳ Built-in functions (math, date, string, list operations)
- ⏳ Cross-node reference resolution  
- ⏳ Dynamic recalculation on data changes

### Phase 7: Advanced Features ⏳ PLANNED
- ⏳ Chart visualization system
- ⏳ Cross-file references and imports
- ⏳ Export/import capabilities
- ⏳ Advanced list operations and aggregations

### Phase 7: User Experience ⏳ PLANNED
- ⏳ Enhanced keyboard shortcuts
- ⏳ Themes and customization
- ⏳ Performance optimizations
- ⏳ Mobile responsive design

## Current Implementation Status

### ✅ Completed Systems
1. **Parser**: Complete nom-based parser supporting all syntax
2. **Resolver**: Parameter inheritance and layout resolution
3. **Renderer**: Advanced HTML generation with styling
4. **Layout Engine**: Horizontal/vertical layouts with spacing/margins
5. **Styling System**: Colors, fonts, borders, sizing with inheritance
6. **Markdown Support**: Rich text with dual edit/view modes
7. **File Operations**: Async I/O with comment preservation
8. **Action System**: Buttons, timers, triggers with safe execution
9. **Chart System**: Chart.js integration with responsive scaling

### 🔄 In Development
1. **Interactive Editing**: Enhanced field validation and editing
2. **List Management**: CRUD operations for dynamic lists
3. **Formula System**: Basic evaluation engine implementation

### ⏳ Planned Systems
1. **Actions/Triggers**: User interaction and automation
2. **Charts**: Data visualization components
3. **Multi-file Support**: Cross-document references

## Performance Characteristics

### Current Benchmarks
- **Parse Time**: ~5ms for typical 1000-line documents
- **Render Time**: ~20ms for complex layouts with 100+ elements
- **Memory Usage**: ~50MB for large documents (10,000+ nodes)
- **File I/O**: Sub-100ms for most document sizes

### Optimization Targets
- **Large Documents**: Support for 50,000+ node documents
- **Real-time Updates**: <16ms render times for smooth interaction
- **Memory Efficiency**: <200MB for any reasonable document size
- **Startup Time**: <2s application launch to ready state

## Security Considerations

### File System Access
- Sandboxed file operations through Tauri
- Explicit permission requests
- User-controlled file selection
- No arbitrary file system access

### Formula Execution
- Sandboxed evaluation environment
- No system command execution
- Limited built-in function set
- Input validation and sanitization

### Data Privacy
- Local-first architecture
- No cloud dependencies by default
- User controls all data storage
- Encryption options for sensitive data

## Performance Considerations

### Parser Optimization
- Incremental parsing for large files
- Lazy evaluation of formulas
- Caching of parsed AST structures
- Memory-efficient data structures

### UI Rendering
- Virtual scrolling for large lists
- Lazy loading of tab content
- Efficient DOM updates
- CSS optimization for animations

### File Operations
- Async I/O throughout
- Background file watching
- Debounced save operations
- Efficient diff algorithms

## Testing Strategy

### Unit Tests
- Parser combinators with edge cases
- Formula evaluation accuracy
- Type system validation
- File operation reliability

### Integration Tests
- End-to-end document workflows
- Cross-file reference resolution
- UI interaction scenarios
- Performance benchmarks

### User Acceptance Tests
- Real-world document creation
- Complex formula scenarios
- Multi-tab navigation
- Mobile responsiveness

## Build and Deployment

### Development Environment
```bash
# Install Rust toolchain
rustup install stable

# Install Node.js dependencies
npm install

# Start development server
npm run tauri dev
```

### Production Build
```bash
# Build for current platform
npm run tauri build

# Cross-compilation targets
cargo tauri build --target x86_64-pc-windows-msvc
cargo tauri build --target x86_64-unknown-linux-gnu
```

### Distribution
- Single executable per platform
- Windows MSI installer
- Linux AppImage/deb packages
- macOS app bundle and DMG
- GitHub Releases for distribution

## Extension Points

### Custom Functions
- Plugin system for formula functions
- JavaScript API for custom evaluators
- Community function marketplace

### Themes and Styling
- CSS variable-based theming
- Custom parameter interpreters
- Layout engine extensions

### Import/Export Formats
- JSON schema compatibility
- CSV data import/export
- Markdown documentation export
- PDF report generation

## Future Considerations

### Performance Scaling
- WebAssembly for compute-intensive operations
- Worker threads for background processing
- Database backend for large datasets
- Streaming for massive files

### Collaboration Features
- Real-time editing (CRDT-based)
- Version control integration
- Conflict resolution strategies
- Team sharing workflows

### Advanced Visualizations
- D3.js integration for custom charts
- Interactive data exploration
- Real-time data streaming
- Machine learning insights

---

This architecture provides a solid foundation for Overseer's development while maintaining flexibility for future enhancements and platform expansion.
