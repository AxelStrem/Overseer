# Dependency Tracking System Implementation Plan

## Current Problem
- Full document re-evaluation on every change causes:
  - UI flickering and scroll reset
  - Performance issues with large documents
  - Field resets due to template re-resolution
  - Poor user experience

## Solution: Reactive Dependency Graph

### Core Components

1. **Dependency Graph**: Track which fields depend on which other fields
2. **Change Propagation**: Only update fields that are actually affected
3. **Smart UI Updates**: Partial DOM updates instead of full re-render
4. **Timer Management**: Track timer states and offline catching-up

### Implementation Strategy

#### Phase 1: Dependency Discovery
- Parse formulas to extract path references
- Build a dependency graph: `field_path -> [dependent_field_paths]`
- Track timer nodes and their triggers
- Identify template inheritance relationships

#### Phase 2: Change Tracking  
- Track which fields have been modified
- Mark dependent fields for re-evaluation
- Batch updates to avoid cascading re-renders

#### Phase 3: Selective Updates
- Replace full document re-parse with targeted field updates
- Update only affected DOM elements
- Preserve scroll position and focus

#### Phase 4: Timer Integration
- Track timer state separately from document state
- Handle offline timer catch-up
- Trigger selective updates when timers fire

### Data Structures

```rust
pub struct DependencyGraph {
    // Maps field path to list of dependent field paths
    dependencies: HashMap<String, Vec<String>>,
    
    // Reverse mapping: dependent -> dependencies
    dependents: HashMap<String, Vec<String>>,
    
    // Timer nodes and their next fire times
    timers: HashMap<String, TimerInfo>,
    
    // Template inheritance relationships
    template_instances: HashMap<String, String>,
}

pub struct TimerInfo {
    pub node_path: String,
    pub next_due: Option<i64>,
    pub interval_ms: Option<i64>,
    pub last_fired: Option<i64>,
}

pub struct FieldUpdate {
    pub path: String,
    pub new_value: OverseerValue,
    pub cascaded: bool, // true if this update was caused by another update
}
```

### Implementation Steps

1. Create dependency tracking module
2. Add dependency discovery during resolver phase
3. Implement change propagation algorithm
4. Replace full re-evaluation with selective updates
5. Integrate with timer system
6. Add comprehensive tests

### Testing Strategy

- Unit tests for dependency graph building
- Integration tests for change propagation
- Timer state management tests
- Template inheritance tests
- Performance benchmarks
