# Overseer Language Syntax Specification

## Overview

Overseer uses a custom domain-specific language (DSL) designed for defining hierarchical data structures with embedded logic, styling, and user interface elements. The syntax emphasizes minimal cognitive overhead while providing powerful features for personal data management.

## Core Philosophy

- **Unified hierarchy**: Data, styling, and logic coexist in the same structure
- **Minimal cognitive overhead**: Reduce the mental effort required to read and write code
- **Human-readable**: Files should be easily editable by technical users
- **Self-contained**: Everything needed for a feature is defined in one place

---

## Basic Syntax Rules

### 1. Node Declaration

```overseer
type NodeName (param=value, param=value) {
    // content
}
```

- **type**: Node type (e.g., `div`, `string`, `list`, `tab`) or `-` for automatically inferred type
- **NodeName**: Optional identifier for the node
- **parameters**: Optional configuration in parentheses
- **content**: Nested nodes or values within braces

#### Type Inference with `-`

The `-` character can be used to automatically infer types in contexts where the type is determinable:

```overseer
// In templated nodes - type inferred from template field
div Task (hidden=true) {
    string name = ""
    int priority = 0
}

list Tasks (entry=../Task) {
    - {  // Type inferred as Task from entry= parameter
        - name = "Complete project"      // Type inferred as string
        - priority = 5                   // Type inferred as int
    }
}

// In lists with vanilla types
list Phases (entry=string) {
    - "Phase 1: Core File Operations"   // Type inferred as string
    - "Phase 2: Basic Content Display"  // Type inferred as string
}
```

#### Examples:
```overseer
// Named node with parameters
div Tasks (background=#F0F0F0, border=solid) {
    // content
}

// Unnamed node
div {
    // content  
}

// Node without parameters
string TaskName {
    // content
}

// Simple data field
int Priority = 5

// Auto-inferred types in list entries
list Items (entry=string) {
    - "First item"    // string inferred from entry=string
    - "Second item"   // string inferred from entry=string
}
```

### 2. Data Types

#### Primitive Types:
- `string`: Text data (single line)
- `text`: Multi-line text with markdown support
- `int`: Integer numbers
- `float`: Floating-point numbers
- `date`: Date values
- `bool`: Boolean true/false
- `enum`: Predefined choice from a list

#### Container Types:
- `div`: Generic container for grouping
- `list`: Collection with CRUD operations and two entry types:
  - **Template node entries**: `list Tasks (entry=../Task)` - structured objects following a template
  - **Vanilla type entries**: `list Names (entry=string)` - simple primitive values
- `tab`: Tab container for UI organization

#### List Entry Types

Lists support two distinct entry patterns:

**Template Node Lists** - for structured data:
```overseer
// Define a template (usually hidden)
div Task (hidden=true) {
    string name = ""
    int priority = 0
    bool completed = false
}

// List using the template
list Tasks (entry=../Task) {
    - {  // Type inferred as Task
        - name = "Complete project"
        - priority = 5
        - completed = false
    }
    - {  // Another Task entry
        - name = "Review code"
        - priority = 3
        - completed = true
    }
}
```

**Vanilla Type Lists** - for simple values:
```overseer
list Phases (entry=string) {
    - "Phase 1: Core File Operations"    // Type inferred as string
    - "Phase 2: Basic Content Display"   // Type inferred as string
    - "Phase 3: Interactive Editing"     // Type inferred as string
}

list Priorities (entry=int) {
    - 1    // Type inferred as int
    - 5    // Type inferred as int
    - 10   // Type inferred as int
}
```

#### Interactive Types:
- `button`: Clickable button with actions
- `checkbox`: Boolean toggle with actions
- `chart`: Data visualization element

#### Logic Types:
- `trigger`: Conditional logic execution
- `action`: Operation to perform

### 3. Value Assignment

```overseer
type FieldName = value
```

#### Examples:
```overseer
string Header = "My Task"
int Priority = 10
date Created = today()
bool Completed = false
enum Status = "Active"  // from predefined choices
```

### 4. Formulas

Formulas use `$()` syntax and are evaluated at runtime:

```overseer
int DaysUntilDue = $(../Due - today())
string Summary = $("Priority: " + ../Priority + ", Due: " + ../Due)
float Average = $(../Tasks/Data.average(Priority))
```

#### Formula Functions:
- `today()`: Current date
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparison: `>`, `<`, `>=`, `<=`, `==`, `!=`
- Conditional: `condition ? value_if_true : value_if_false`
- String concatenation: `+`

### 5. Path References

Use filesystem-like paths to reference other nodes:

```overseer
../Task              // Parent's Task child
../../Statistics     // Grandparent's Statistics child
./Data/5/Header      // Local path to 5th item's Header
../file.os/Tasks     // Reference to another file
```

### 6. Array Indexing

Use square brackets for accessing array elements:

```overseer
string FirstTask = $(../Tasks/Data[0]/Header)
int HighestPriority = $(../Tasks/Data.find_max(Priority)[0]/Priority)
```

### 7. List Operations

Lists provide built-in query methods:

```overseer
count()                    // Number of items
where(condition)           // Filter items
sort(field)               // Sort by field
sort_decreasing(field)    // Sort descending
find_max(field)           // Find maximum value
find_min(field)           // Find minimum value
average(field)            // Calculate average
sum(field)                // Calculate sum
```

#### Examples:
```overseer
int ActiveTasks = $(../Tasks/Data.where(Completed == false).count())
int HighestPriority = $(../Tasks/Data.find_max(Priority)/Priority)
float AvgPriority = $(../Tasks/Data.average(Priority))
```

---

## Advanced Features

### 1. Templates and Inheritance

Use hidden template sections and `base` parameter for reusability:

```overseer
tab Tasks {
    div (hidden=true) {  // Template section
        div Task (background=$(Priority>20?Red:White)) {
            string Header = ""
            int Priority = 0
            date Due = ""
            bool Completed = false
        }
    }
    
    list Data (entry=../Task) {
        div task1 (base=../../Task) {
            Header = "Learn Overseer"
            Priority = 5
            Due = "2025-07-30"
        }
    }
}
```

### 2. Actions and Triggers

#### Actions
Actions perform operations when triggered:

```overseer
action ActionType (target=path, param=value) = expression

// Action types:
action Set (target=../Priority) = $(../Priority + 1)
action Add (target=../Counter) = 1
action Create (target=../NewList, template=../ItemTemplate)
action Delete (target=../Items/5)
```

#### Triggers
Triggers execute actions based on conditions:

```overseer
trigger (condition=$(../DaysUntilDue < 2)) {
    action Set (target=../Priority) = $(../Priority + 10)
}
```

#### Interactive Elements with Actions
```overseer
checkbox Completed {
    action Set (target=../Completed) = true
    action Add (target=../../Statistics/CompletedTasks) = 1
}

button DeleteTask "Delete" {
    action Delete (target=../)
}
```

### 3. Layout and Styling

#### Layout Parameters:
```overseer
div Content (horizontal-size=30%, border=Right, background=#F0F0F0) {
    // content
}

list TaskList (layout=grid, sort=Priority, direction=horizontal) {
    // list items
}

tab Projects (direction=vertical, position=left) {
    // tab content
}
```

#### Chart Configuration:
```overseer
chart WeightChart (
    type=line, 
    data=../WeightData/*/Weight, 
    labels=../WeightData/*/Date,
    title="Weight Progress",
    color=#0066CC
) {
    // chart styling options
}
```

---

## Complete Example

```overseer
// File: personal_dashboard.os

tab Dashboard {
    div Statistics (background=#F5F5F5, padding=10px) {
        int TotalTasks = $(../Tasks/Data.count())
        int CompletedTasks = $(../Tasks/Data.where(Completed == true).count())
        float CompletionRate = $(CompletedTasks / TotalTasks * 100)
        
        text Summary = $("Completed " + CompletedTasks + " out of " + TotalTasks + " tasks (" + CompletionRate + "%)")
    }
}

tab Tasks {
    // Hidden template section
    div (hidden=true) {
        div Task (background=$(Priority>20?"#FFE6E6":"#E6F3FF")) {
            string Header = ""
            text Description = ""
            int Priority = 0
            date Created = today()
            date Due = ""
            int DaysUntilDue = $(../Due - today())
            bool Completed = false
            
            // Automatic priority boost for urgent tasks
            trigger (condition=$(../DaysUntilDue < 2 && ../Completed == false)) {
                action Set (target=../Priority) = $(../Priority + 10)
            }
            
            // Completion checkbox
            checkbox Completed {
                action Set (target=../Completed) = true
                action Add (target=../../Statistics/CompletedTasks) = 1
            }
            
            // Priority adjustment buttons
            button BumpPriority "↑" {
                action Set (target=../Priority) = $(../Priority + 1)
            }
            
            button LowerPriority "↓" {
                action Set (target=../Priority) = $(../Priority - 1)
            }
            
            button DeleteTask "Delete" {
                action Delete (target=../)
            }
        }
    }
    
    // Task list with grid layout
    list Data (entry=../Task, layout=grid, sort=Priority, direction=vertical) {
        div urgent_task (base=../../Task) {
            Header = "Fix critical bug"
            Description = "System crashes on startup"
            Priority = 25
            Due = "2025-07-25"
        }
        
        div normal_task (base=../../Task) {
            Header = "Write documentation"
            Description = "Update API documentation"
            Priority = 5
            Due = "2025-07-30"
        }
        
        div completed_task (base=../../Task) {
            Header = "Setup development environment"
            Priority = 3
            Due = "2025-07-20"
            Completed = true
        }
    }
    
    // Add new task button
    button AddTask "Add New Task" {
        action Create (target=Data, template=../Task)
    }
}

tab Analytics {
    chart PriorityDistribution (
        type=bar,
        data=../Tasks/Data.group_by(Priority).count(),
        title="Tasks by Priority Level",
        color=#4A90E2
    )
    
    chart CompletionTrend (
        type=line,
        data=../Statistics/CompletionHistory/*/Rate,
        labels=../Statistics/CompletionHistory/*/Date,
        title="Completion Rate Over Time"
    )
}
```

---

## File Organization

### Single File
```overseer
// simple_tasks.os
div Tasks {
    // all content in one file
}
```

### Multi-File References
```overseer
// main.os
div Dashboard {
    int TaskCount = $(../tasks.os/Tasks/Data.count())
}

// tasks.os
div Tasks {
    // task definitions
}
```

### Hidden Cache Directory
```
project/
├── main.os
├── tasks.os
├── statistics.os
└── .overseer/
    ├── cache.json
    └── computed_values.json
```

---

## Comments

```overseer
// Single line comment

/*
Multi-line comment
Can span multiple lines
*/

div Tasks {
    string Header = "Task"  // Inline comment
    
    /*
    This is a template for tasks
    Used by the list below
    */
    div (hidden=true) {
        // template content
    }
}
```

---

## Reserved Keywords

- Node types: `div`, `string`, `text`, `int`, `float`, `date`, `bool`, `enum`, `list`, `tab`, `button`, `checkbox`, `chart`, `trigger`, `action`
- Parameters: `hidden`, `background`, `border`, `layout`, `sort`, `direction`, `base`, `entry`, `target`, `condition`, `type`, `data`, `labels`
- Functions: `today()`, `count()`, `where()`, `sort()`, `find_max()`, `find_min()`, `average()`, `sum()`
- Operators: `+`, `-`, `*`, `/`, `%`, `>`, `<`, `>=`, `<=`, `==`, `!=`, `&&`, `||`, `!`
- Literals: `true`, `false`

---

This specification provides the foundation for implementing the Overseer language parser and runtime engine.
