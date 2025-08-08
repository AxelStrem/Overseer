# Formula System Implementation Plan

## Overview
Implement `$(expression)` formula evaluation system for Overseer DSL with arithmetic operations, field references, and built-in functions.

## Key Requirements
- **Evaluation Timing**: After template resolution, before rendering (crucial for relative paths in templated nodes)
- **Error Handling**: Display "invalid formula error" instead of crashing
- **Caching**: Cache results with future observer pattern for triggers/actions
- **Incremental Complexity**: Start simple, add features progressively

## Phase 1: Core Formula Evaluation Engine

### 1.1 Architecture Setup
- [x] Create `formula_evaluator.rs` module in Rust backend
- [x] Add evaluation context struct to track current node position in document tree
- [x] Integrate evaluation into resolver after template expansion (basic integration)
- [ ] Add formula result caching infrastructure

### 1.2 Basic Expression Parser
- [x] Parse numeric literals (integers and floats)
- [x] Parse arithmetic operators: `+`, `-`, `*`, `/`
- [x] Parse parentheses for grouping: `(expression)`
- [x] Implement operator precedence (*, / before +, -)
- [x] Handle whitespace in expressions

### 1.3 Error Handling
- [x] Graceful error handling for parse failures
- [x] Return "invalid formula error" message
- [x] Continue document processing despite formula errors (basic)

## Phase 2: Field References

### 2.1 Simple Relative Path Navigation
- [x] Parse `../field` syntax for parent node field access (parsing only)
- [x] Parse `../../field` syntax for grandparent access
- [ ] Implement document tree traversal from current node
- [x] Handle missing/invalid paths gracefully

### 2.2 Field Value Extraction
- [ ] Extract values from referenced nodes
- [ ] Handle different value types (String, Integer, Float, Boolean)
- [ ] Type coercion for arithmetic operations (string to number)

## Phase 3: Built-in Functions

### 3.1 Date Functions
- [x] Implement `today()` function returning current date
- [x] Format date appropriately for Overseer date type (YYYY-MM-DD)

## Phase 4: Advanced Features (Future Steps)

### 4.1 Conditional Expressions
- [x] Ternary operator: `condition ? value_if_true : value_if_false`
- [x] Comparison operators: `==`, `!=`, `<`, `>`, `<=`, `>=`
- [x] Boolean logic: `&&`, `||`, `!`

### 4.2 List Operations
- [ ] Numeric indices: `../list[10]`
- [x] Parameter extraction: `../field.color`
- [ ] Aggregate functions: `sum()`, `count()`, `avg()`

### 4.3 Observer Pattern (for triggers/actions)
- [ ] Track formula dependencies
- [ ] Invalidate cached results when dependencies change
- [ ] Re-evaluate affected formulas

## Implementation Strategy

### Step-by-Step Approach
1. **Start with arithmetic**: `$(5 + 3 * 2)` → `11`
2. **Add field references**: `$(salary + bonus)` → `6000` (pending resolution logic)
3. **Add today() function**: `$(today())` → `"2025-08-07"`
4. **Add path navigation**: `$(../total_income - expenses)` (pending traversal logic)

### Test Cases to Support
```overseer
div test_formulas {
    int a = 10
    int b = 5
    
    // Arithmetic
    int sum = $(a + b)           // → 15
    int complex = $(a + b * 2)   // → 20 (precedence)
    int parens = $((a + b) * 2)  // → 30 (grouping)
    
    // Field references
    div parent {
        int income = 5000
        div child {
            int bonus = 1000
            int total = $(income + bonus)     // → 6000 (parent field)
            int double = $(../income * 2)     // → 10000 (explicit path)
        }
    }
    
    // Built-in functions
    date updated = $(today())    // → "2025-08-07"
}
```

### Error Cases to Handle
```overseer
div error_tests {
    int a = 10
    
    int bad_syntax = $(a +)           // → "invalid formula error"
    int missing_field = $(nonexistent) // → "invalid formula error"
    int bad_path = $(../../../missing) // → "invalid formula error"
}
```

## Integration Points

### Rust Backend
- `src-tauri/src/formula_evaluator.rs` - New module
- `src-tauri/src/resolver.rs` - Call evaluator after template resolution
- `src-tauri/src/types.rs` - Add FormulaError variant if needed

### Frontend
- No changes needed initially (formulas resolve to concrete values)
- Later: Display error messages for failed formulas

## Success Criteria
- [x] Basic arithmetic works: `$(5 + 3)` → `8`
- [ ] Field references work: `$(salary + bonus)` 
- [ ] Parent navigation works: `$(../parent_field)`
- [x] today() function works: `$(today())` → current date
- [x] Errors are handled gracefully without crashes
- [ ] Integration with existing example.os formulas

## Next Steps
1. Implement field reference resolution within the current node
2. Implement `../` path traversal (and `../../` etc.) with ancestor navigation in EvaluationContext
3. Add unit/DSL tests exercising field and path references
4. Add boolean logic (&&, ||, !) and ternary operator parsing/evaluation
5. Consider result caching and dependency tracking for observer pattern
