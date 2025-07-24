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

### 3. Formula Evaluator (`evaluator.rs`)
**Purpose**: Process and evaluate $() formulas within documents

**Features**:
- Expression parsing and evaluation
- Built-in functions (today(), sum(), avg(), etc.)
- Path resolution for cross-node references
- List operations and aggregations
- Type coercion and validation

### 4. File Operations (`file_ops.rs`)
**Purpose**: Handle all file system interactions asynchronously

**Operations**:
- Read/write .os files
- Directory scanning for Overseer documents
- File watching for live updates
- Import/export functionality
- Backup and versioning support

### 5. Renderer (`renderer.js`)
**Purpose**: Generate interactive UI from parsed AST

**Capabilities**:
- Dynamic HTML generation from node hierarchy
- CSS styling based on parameters
- Interactive editing (double-click to edit)
- Tab management and navigation
- Chart rendering integration

## Data Flow

### Document Loading Process
1. User selects .os file through UI
2. Frontend calls `load_overseer_file` Tauri command
3. Backend reads file asynchronously
4. Parser converts content to AST
5. AST returned to frontend via IPC
6. Renderer generates HTML from AST
7. Interactive UI displayed to user

### Formula Evaluation Workflow
1. Parser identifies $() formulas during parsing
2. Evaluator resolves references and dependencies
3. Built-in functions executed with current context
4. Results cached for performance
5. UI updated with calculated values
6. Re-evaluation triggered on data changes

### File Saving Process
1. User modifies data through UI
2. Frontend updates local AST representation
3. Save command serializes AST back to DSL syntax
4. Backend writes content to file system
5. File watchers notify of changes
6. Auto-save and backup procedures

## DSL Syntax Specification

### Node Types
- **tab**: Top-level navigation containers
- **div**: Generic containers with layout options
- **list**: Collections with iteration support
- **string**: Text input fields
- **text**: Multi-line text with markdown support
- **int/float**: Numeric input fields
- **date**: Date picker components
- **bool**: Checkbox/toggle controls
- **button**: Interactive action triggers
- **chart**: Data visualization components

### Parameter System
Parameters modify node behavior and appearance:
```
node_name(parameter_key=parameter_value, style=modern) {
    content...
}
```

### Formula Language
Formulas enable dynamic calculations:
```
field_name = $(today() + 30)
total = $(sum(expenses.amount))
average = $(avg(scores[]))
```

## Development Phases

### Phase 1: Foundation (Current)
- ✅ Project structure setup
- ✅ Basic parser implementation
- ✅ File I/O operations
- ✅ Simple UI rendering
- 🔄 Complete syntax support

### Phase 2: Core Functionality
- Formula evaluation engine
- CRUD operations for all field types
- Tab navigation system
- Basic chart integration
- Error handling and validation

### Phase 3: Advanced Features
- Action/trigger system
- Cross-file references
- Advanced chart types
- Export/import capabilities
- Performance optimizations

### Phase 4: User Experience
- Rich text editing
- Drag-and-drop interface
- Keyboard shortcuts
- Themes and customization
- Documentation and tutorials

### Phase 5: Mobile & Sync
- Android application (Tauri mobile)
- Cloud synchronization
- Offline capabilities
- Multi-device support

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
