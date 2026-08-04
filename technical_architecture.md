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
- **regex**: Advanced formula parsing for dependency tracking

#### Frontend (JavaScript)
- **Vite**: Modern build tool and development server
- **Chart.js 4.0.0**: Advanced data visualization with full chart support
- **Native DOM**: Direct HTML/CSS manipulation with surgical updates

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    Frontend (Web)                       │
├─────────────────────────────────────────────────────────┤
│  UI Layer: HTML/CSS/JavaScript                         │
│  - Document Renderer (Selective DOM Updates)          │
│  - Interactive Editor (Field-level Updates)           │
│  - Chart Visualizations (Chart.js Integration)        │
│  - Cascade Detection & DOM Management                 │
│  - File Management Interface                          │
└─────────────────────┬───────────────────────────────────┘
                      │ Tauri IPC (Selective Updates)
┌─────────────────────▼───────────────────────────────────┐
│                  Backend (Rust)                        │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │   Parser    │  │  Evaluator  │  │ File Ops    │     │
│  │   (nom)     │  │  (formulas) │  │ (async I/O) │     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
│                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │Dependency   │  │   Actions   │  │   Storage   │     │
│  │  Tracker    │  │ (triggers)  │  │    (.os)    │     │
│  │ (Selective) │  └─────────────┘  └─────────────┘     │
│  └─────────────┘                                       │
│                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │    Types    │  │  Resolver   │  │   Chart     │     │
│  │    (AST)    │  │ (Selective) │  │ Integration │     │
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

### 6. Dependency Tracker Module (`dependency_tracker.rs`) - **NEW**
**Purpose**: Advanced dependency tracking and selective update coordination

**Key Features**:
- Regex-based formula parsing to extract field references
- Sophisticated dependency graph construction with bidirectional mapping
- Context-aware path resolution (sibling fields, hierarchical references)
- Timer node tracking for future timer system integration
- Cascade calculation for formula dependencies

**Core Functions**:
```rust
impl DependencyGraph {
    pub fn build_dependencies(nodes: &[OverseerNode]) -> Result<Self, OverseerError>
    pub fn get_affected_fields(&self, changed_fields: &[String]) -> Vec<String>
    pub fn extract_path_references(formula: &str) -> Vec<String>
    pub fn resolve_path_reference(reference: &str, context: &str) -> String
}
```

### 7. Enhanced Resolver Module (`resolver.rs`) - **UPDATED**
**Purpose**: Selective field resolution and intelligent chart computation

**New Capabilities**:
- `resolve_specific_fields`: Surgical backend processing for affected fields only
- Field value preservation during selective updates
- Smart chart data computation that skips unnecessary generation
- Integration with dependency tracker for cascade processing

**Performance Features**:
- Eliminates full document re-parsing
- Maintains field state during backend processing
- Intelligent chart series computation based on actual dependencies

### 8. Renderer (`renderer.js`) - **ENHANCED**
**Purpose**: Generate interactive UI from parsed AST with advanced styling and selective updates

**Capabilities**:
- Dynamic HTML generation from node hierarchy
- Advanced CSS styling (colors, fonts, borders, sizing)
- Layout system (horizontal/vertical with spacing/margins)
- Markdown rendering with dual edit/view modes
- Interactive editing with field-level validation
- Tab management and navigation
- Parameter inheritance visualization
- Surgical DOM updates for individual fields
- Cascade field detection and management
- Chart.js 4.0.0 integration with full chart support
- Intelligent chart refresh prevention

### 9. Chart Integration System
**Purpose**: Complete Chart.js 4.0.0 integration with performance optimization

**Key Features**:
- Full chart rendering pipeline (line, bar, scatter, area charts)
- Intelligent chart refresh prevention system
- Event emission control to prevent refresh loops
- Data series computation with dependency analysis
- Chart stability during unrelated field changes
- Advanced chart configuration and data binding

**Integration Points**:
- Backend chart series generation with formula evaluation
- Frontend Chart.js initialization and lifecycle management
- Selective chart data refreshing based on actual dependencies
- Chart type detection and automatic configuration

## Data Flow

### Document Loading Process
1. User selects .os file through UI
2. Frontend calls `load_overseer_file` Tauri command
3. Backend reads file asynchronously
4. Parser converts content to AST
5. Resolver processes inheritance, layout, and parameters
6. **NEW**: Dependency tracker builds formula dependency graph
7. AST returned to frontend via IPC
8. Renderer generates HTML with advanced styling and Chart.js integration
9. Interactive UI displayed to user with full chart support

### Enhanced Formula Evaluation Workflow
1. Parser identifies $() formulas during parsing
2. Dependency tracker extracts field references and builds dependency graph
3. Resolver prepares formula context and dependencies
4. Evaluator resolves references and executes calculations
5. Built-in functions executed with current context
6. Results cached for performance
7. **NEW**: Multi-tier selective update system determines update strategy:
   - **Tier 1**: DOM-only updates for simple field changes
   - **Tier 2**: Selective backend processing for formula dependencies
   - **Tier 3**: Cascade detection and DOM updates for dependent fields
   - **Tier 4**: Intelligent chart refresh prevention
8. UI updated with calculated values (surgical DOM updates)
9. Charts updated only when their data dependencies change

### Selective Update Process
1. User modifies field in UI
2. Frontend detects field change and determines update strategy
3. For simple changes: DOM-only update preserves scroll position
4. For formula dependencies:
   a. Backend processes only affected fields using dependency graph
   b. Field values preserved during backend processing
   c. Cascade changes detected by comparing old vs new document
   d. DOM surgically updated for both direct and cascade changes
5. Charts refresh only if their data sources actually changed
6. User experience: Instant responsive updates without flicker

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

## Development Phases and Status

Tracked in [DEVELOPMENT_PLAN.os](DEVELOPMENT_PLAN.os), with open defects and
requested features in [KNOWN_BUGS.os](KNOWN_BUGS.os). Deliberately not restated
here: this document describes how the system is built, which changes far more
slowly than what is done.

## Performance Characteristics

The architectural answer to performance is the dependency tracker: an edit
recomputes the formulas that transitively depend on the changed node, and the
renderer patches the corresponding DOM subtrees rather than re-rendering the
document. Full re-evaluation is the fallback path, not the normal one.

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
