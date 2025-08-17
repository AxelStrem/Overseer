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

### 4.2 List Operations — List Pipeline (map/filter + aggregates)

Goal: Support concise, chainable list processing with per-item expressions.

Syntax overview
- Chaining via dot: `ReceiverExpr . method ( args? ) …`
- Supported methods (MVP): `map(expr)`, `filter(predicate)`, `sum()`, `count()`, `max()`, `min()` (optional: `avg()`)
- Examples:
    - `$(/Items.map(|x| x/amount).sum())`
    - `$(/Items.filter(|x| (x/flags/complete) && (x/flags/tested)).count())`
    - `$(/Tasks.map(|t| t/estimated-hours * 60).max())`
    - `$(/Orders.filter(|o| o/status == "open").map(|o| o/total).sum())`

Field/Path semantics inside map/filter (lambda-only)
- Lambdas are required for `map` and `filter`.
- Inside a lambda, the item parameter (e.g., `x`) acts as the anchor for navigation:
    - `x/seg1/…/segN` navigates within the item.
    - Resolution rule for the final segment `segN` on the targeted node:
        1) If a parameter named `segN` exists, return that parameter’s value (computed-first).
        2) Else if a child node named `segN` exists, return that node (for further navigation/evaluation).
        3) Else → null.
    - Intermediate segments (`seg1 … segN-1`) navigate children by name only.
- Outside lambdas, existing anchors remain unchanged:
    - `.param` and `/path` keep current behavior for non-lambda expressions.

Reserved identifiers / name collisions
- Reserved method names: `map, filter, sum, count, max, min, avg, reduce`.
- These should be avoided as lambda parameter names to reduce confusion (not strictly forbidden).
- Parameter vs child name conflicts at the terminal segment are resolved with the param-first rule above; consider adding an explicit disambiguator later if needed.

Type and error semantics
- `map(expr)` → returns a list (vector) of evaluated results; if an item’s expr errors, result is `null` for that item.
- `filter(predicate)` → returns a list of items where predicate is truthy; predicate errors are treated as `false` (item excluded).
- Aggregates consume lists:
    - `count()` → length of input list (after filters); counts items regardless of value type.
    - `sum()` → sums only numeric entries (integers/floats); non-numeric/null ignored; empty → `0`.
    - `max()/min()` → numeric only; non-numeric/null ignored; empty → `null`.
    - `avg()` (if implemented) → numeric average; empty or no numeric values → `null`.
- Computed-first lookup applies when reading parameters.

Operator precedence
- Method chaining binds tighter than multiplicative/additive ops:
    - `$(/Items.map(|x|x).sum() * 2)` is parsed as `( (/Items).map(|x|x).sum() ) * 2`.

Implementation tasks
1) Parser (src-tauri/src/parser.rs)
     - Extend expression grammar: `primary ('.' IDENT '(' argExpr? ')')*`.
     - Parse a single expression as the argument to `map(...)` and `filter(...)`.
     - Ensure method-call chain has higher precedence than `* / + -` but below postfix parentheses grouping.
     - Reuse existing expression parser for `argExpr` so full formula syntax is supported.

2) Evaluator (src-tauri/src/formula_evaluator.rs)
     - Represent method chains in the AST (e.g., a Vec of MethodCall on a base Expr).
     - Support lambda literals with parameter list and body.
     - Evaluate base list expression → resolve to a list of item nodes.
     - For `map(lambda)`/`filter(lambda)`:
         - Iterate list items; bind the lambda’s last parameter to the current item node in a new lexical scope.
         - Evaluate the lambda body; support variable-anchored paths `ident/seg/…` using the resolution rules.
     - For `reduce(init, lambda)`:
         - Initialize accumulator from `init`; iterate items binding `(acc, x)`; compute new `acc` from lambda result.
     - Aggregates (`sum/count/max/min/avg`) consume lists per the semantics above.
     - Non-list receivers: return `null` for the whole chain.

3) Resolver integration (src-tauri/src/resolver.rs)
     - No order changes; formulas still evaluate after template/layout/param inheritance.
     - Ensure per-instance EvaluationContext works with list anchors (already supported by `new_with_current`).

4) Types and conversions (src-tauri/src/types.rs)
     - Ensure evaluator can carry list-of-values internally (no public type change required unless exposing lists elsewhere).
     - Numeric coercion rules: integers/floats normalized for aggregates; non-numeric ignored by numeric aggregates.

5) Frontend (no UI change required)
    - Renderer already prefers computed values; list pipeline results will be numbers/strings used by nodes.

6) Tests
     - Happy paths:
         - Sum: `$(/Items.map(|x| x/amount).sum())` → correct total.
         - Count with condition: `$(/Items.filter(|x| x/ok).count())` → correct count.
         - Max after projection: `$(/Tasks.map(|t| t/hours * 60).max())` → correct max.
     - Edge cases:
         - Empty list → sum=0, count=0, max/min/avg=null.
         - Non-numeric entries in map → ignored by sum/max/min/avg.
         - Predicate error → treated as false (excluded).
         - Non-list receiver → null.

7) Performance & safety
     - Short-circuit where possible: `count()` can skip building intermediate lists if previous stage is a filter only.
     - Keep evaluation non-destructive; store results in `_computed_*` as needed by callers.

Notes / Future Enhancements
- Add `reduce(init, expr)` for custom folds.
- Add `distinct()` / `sortBy(expr)` as needed later.
- Consider escape syntax for `.param` names colliding with method names if user scenarios demand it.

### 4.2.1 Lambda expressions (for map/filter/reduce) — lambda-only map/filter

Goal: First-class, lightweight lambdas to make list pipelines consistent and enable custom folds.

Syntax
- Lambda literal: `|x| expr` (one-arg) or `|acc, x| expr` (two-arg)
    - Examples:
        - `$(/Items.map(|x| x/amount).sum())`
        - `$(/Items.filter(|x| (x/flags/complete) && (x/flags/tested)).count())`
        - `$(/Tasks.map(|t| t/estimated-hours * 60).max())`
        - `$(/Items.reduce(0, |acc, x| acc + x/amount))`

Semantics
- Parameter binding:
    - Names in `|…|` are bound as lambda variables in a new lexical scope.
    - For `map`/`filter`: one param (e.g., `x`) receives the current list item node.
    - For `reduce(init, lambda)`: two params `(acc, x)` receive the accumulator and current item respectively.
- Variable-anchored paths:
    - `ident/seg1/…/segN` resolves starting at the node bound to `ident`.
    - Terminal segment param-first lookup; intermediate segments navigate child nodes.
    - Computed-first applies when retrieving parameters.
- Variables in expressions:
    - Lambda parameters (e.g., `acc`, `x`) can be referenced directly.
    - `x.amount` style access is not part of MVP; prefer `x/amount`.
- Backward-compat:
    - `map` and `filter` require lambdas (no implicit `map(expr)`/`filter(expr)` sugar).

Reserved identifiers / collisions
- Extend reserved method names set to remain disjoint from `.param` names and typical lambda param names:
    - Methods: `map, filter, sum, count, max, min, avg, reduce`
    - Recommended to avoid using these as `.param` names; no change for bare values.

Implementation tasks (Lambda)
1) Parser (src-tauri/src/parser.rs)
     - Add lambda literal production: `lambda := '|' ident (',' ident)* '|' expression`.
     - Recognize variable-anchored paths: `varPath := ident ('/' segment)*` where `segment` supports kebab-case.
     - Allow lambda as a valid `argExpr` in method calls.
     - Maintain method-call chain precedence as previously defined.
2) Evaluator (src-tauri/src/formula_evaluator.rs)
     - Represent lambda in AST with parameter list and body expression.
     - Add lexical scope to `EvaluationContext` to bind parameter names to runtime values.
     - Support evaluation of `varPath` by anchoring resolution to the node bound to the variable.
     - For `map(lambda)` / `filter(lambda)` / `reduce(init, lambda)`:
         - Iterate list items; bind parameters and evaluate body with variable-anchored paths.
         - `reduce`: initialize accumulator with evaluated `init`; fold via lambda; return final accumulator.
     - No implicit sugar for map/filter.
3) Tests
     - `map(|x| x/amount).sum()` produces correct total.
     - `filter(|x| x/ok).count()` produces correct count.
     - `reduce(0, |acc, x| acc + x/amount)` sums a numeric field.
     - Variable-anchored paths resolve correctly; terminal param-first rule verified.
     - Non-list receivers produce `null`.

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
- [x] Field references work: `$(salary + bonus)` 
- [x] Parent navigation works: `$(../parent_field)`
- [x] today() function works: `$(today())` → current date
- [x] Errors are handled gracefully without crashes
- [x] Integration with existing example.os formulas

## Next Steps
1. Add unit/DSL tests exercising field and path references
2. Add boolean logic (&&, ||, !) and ternary operator parsing/evaluation
3. Consider result caching and dependency tracking for observer pattern

Example:
 tab CounterDemo { 
    div CounterBox (layout="horizontal", spacing=8) {       
        int counter = 0
        button Increment (label="Increment") { 
            // Event block attached to the button 
            on click { 
                // Increment the sibling counter field by 1 
                inc (path="../counter", by=1)
                 } 
                }
             } 
            }