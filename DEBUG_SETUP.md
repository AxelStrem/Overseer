# Debug Logging Setup

This document explains how to enable and use debug logging features in the Overseer project.

## Quick Start

### Run with Debug Logging
```bash
# Enable all debug logging (parser + resolver + evaluator)
npm run tauri:dev:debug

# Enable only parser debug logging
npm run tauri:dev:debug-parser

# Enable only resolver debug logging  
npm run tauri:dev:debug-resolver

# Enable only evaluator debug logging
npm run tauri:dev:debug-evaluator

# Run without debug logging (production mode)
npm run tauri:dev
```

### Build with Debug Logging
```bash
# Build with debug features enabled
npm run tauri:build:debug

# Build without debug features (production)
npm run tauri:build
```

## Debug Features

### Parser Debug (`debug-parser`)
When enabled, shows:
- `[PARSER]` Input being parsed
- `[PARSER]` Invalid node starts being skipped
- Parser processing flow

### Resolver Debug (`debug-resolver`) 
When enabled, shows:
- `[RESOLVER]` Template discovery and mapping
- `[RESOLVER]` Template field definitions
- `[RESOLVER]` Node resolution process
- `[RESOLVER]` List item processing
- `[RESOLVER]` Template merging and field overrides
- `[RESOLVER]` Type inference for `-` types

### Evaluator Debug (`debug-evaluator`)
When enabled, shows:
- `[EVAL]` Entry/exit for each formula evaluation with node path
- `[EVAL]` Parsed AST for expressions
- `[EVAL]` Path resolution attempts and matches (field, /-anchored, ../, identifier)
- `[EVAL]` Lambda bindings and method chain steps (map/filter/reduce/aggregates)
- `[EVAL]` Errors with context

## Technical Implementation

### Cargo Features
```toml
[features]
debug-parser = []
debug-resolver = []
debug-evaluator = []
```

### Debug Macros
```rust
// Parser debug macro
debug_parser!("[PARSER] {}", message);

// Resolver debug macro  
debug_resolver!("[RESOLVER] {}", message);

// Evaluator debug macro
debug_evaluator!("[EVAL] {}", message);
```

### Feature-Gated Compilation
Debug statements are only compiled when the corresponding feature flag is enabled:
```rust
macro_rules! debug_parser {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-parser")]
        println!($($arg)*);
    };
}
macro_rules! debug_evaluator {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-evaluator")]
        println!($($arg)*);
    };
}
```

## Output Examples

### Parser Debug Output
```
[PARSER] input: tab (title="1.1: Test Current File Operations") {
[PARSER] input:     text StepsHeader = "Current Development Plan - Short Version"
```

### Resolver Debug Output
```
[RESOLVER] Building template map from 1 root nodes
[RESOLVER] Found template: Task with 4 children
[RESOLVER]   Template field: task_description (type: string)
[RESOLVER]   Template field: complete (type: checkbox)
[RESOLVER] Template map built with 2 templates: ["Task", "Step"]
[RESOLVER] Resolving node: main (type: tab)
[RESOLVER] List Steps uses template: ../Step
[RESOLVER]   Processing list item 0: - (type: -)
[RESOLVER]     Complex list item with 4 children
[RESOLVER]     Override fields: ["priority", "StepTasks", "description", "name"]
[RESOLVER]   Merging field: name (template type: string, override type: -)
[RESOLVER]     Setting value: String("1.1: Test Current File Operations")
```

## Benefits

1. **Performance**: Zero runtime overhead when debug features are disabled
2. **Selective Debugging**: Enable only the logging you need  
3. **Clean Production Builds**: No debug output in release builds
4. **Development Efficiency**: Comprehensive logging for troubleshooting parsing and template resolution issues

## Recent Fixes Applied

This debug system helped identify and fix:
- ✅ Parser premature type inference issue
- ✅ Hidden template nodes visibility bug  
- ✅ Resolver list item recognition failure
- ✅ Template field merging and type resolution

With debug logging, complex parsing and resolution issues can be quickly diagnosed and resolved.
