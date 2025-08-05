# Testing Guide

## Overview
Our testing setup includes comprehensive unit tests for the core Overseer language parsing and resolution functionality.

## Test Structure

### Parser Tests (8 tests)
Located in `src-tauri/src/parser.rs`:
- `test_parse_simple_node` - Basic node parsing
- `test_parse_node_with_block` - Block structure parsing
- `test_parse_boolean_and_float_values` - Value type parsing
- `test_parse_list_with_complex_items` - List parsing with complex items
- `test_parse_templated_node` - Template reference parsing
- `test_parse_layout_parameters` - Layout-specific parameter parsing
- `test_parse_node_with_hyphenated_parameters` - Hyphenated parameter support
- `test_parse_complex_parameters` - Complex nested parameter syntax

### Resolver Tests (6 tests)
Located in `src-tauri/src/resolver.rs`:
- `test_layout_resolution_vertical` - Vertical layout resolution
- `test_layout_resolution_horizontal` - Horizontal layout resolution  
- `test_layout_resolution_inherit` - Layout inheritance behavior
- `test_layout_resolution_opposite` - Opposite layout behavior
- `test_layout_resolution_default_alternation` - Default layout alternation
- `test_template_resolution` - Template instantiation and resolution

## Running Tests

### Command Line Options

```bash
# Run all tests
npm test

# Run only Rust tests  
npm run test:rust

# Run tests in watch mode (continuous)
npm run test:rust:watch

# Run tests with verbose output
npm run test:verbose

# Run from Rust project directly
cd src-tauri
cargo test

# Run specific test
cargo test test_name

# Run with debug output
cargo test test_name -- --nocapture
```

### VS Code Integration

Use the Command Palette (Ctrl+Shift+P):
- "Tasks: Run Task" → "Test: Run All Tests"
- "Tasks: Run Task" → "Test: Run Rust Tests" 
- "Tasks: Run Task" → "Test: Watch Rust Tests"

## Test Coverage

**Current Status: 14/14 tests passing** ✅

- **Parser Coverage**: All major parsing features including layout parameters, templates, complex syntax
- **Resolver Coverage**: Layout resolution logic, template instantiation
- **Integration**: Tests work together to validate end-to-end DSL processing

## Continuous Testing

The `cargo-watch` tool is installed for continuous testing during development:

```bash
cd src-tauri
cargo watch -x test
```

This will automatically re-run tests when source files change.

## Notes

- Template resolution currently only copies overridden fields from templates (not all template fields)
- Layout resolution defaults root nodes to horizontal layout
- All unused variable warnings have been cleaned up for cleaner test output
