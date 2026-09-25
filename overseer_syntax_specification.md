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

- **type**: Node type (e.g., `div`, `string`, `list`, `tab`, `int`, `float`) or `-` for automatically inferred type
- **NodeName**: Optional identifier for the node; Nodes with no name are transparent for the hierarchy, their children can be accessed as if they are immediate children of the parent node.
- **parameters**: Optional configuration in parentheses
- **content**: Nested nodes or values within braces

#### Type Inference with `-`

The `-` character can be used to automatically infer types in contexts where the type is determinable:

```overseer
// In templated nodes - type inferred from template field
div TaskTemplate (hidden=true) {
    string name = ""
    int priority = 0
}

list Tasks (entry=<../TaskTemplate>) {
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

// Node created from a template
<../TaskTemplate> my_task {
    - name = "A new templated task"
}
```

### 2. Data Types

#### Primitive Types:
- `string`: Text data (single line)
- `textbox`: A box to type into - see "A form that fills an entry"
- `text`: Multi-line text with markdown support
- `int`: Integer numbers
- `float`: Floating-point numbers
- `date`: Date values
- `bool`: Boolean true/false
- `enum`: Predefined choice from a list

#### Container Types:
- `div`: Generic container for grouping and visual placement and styling
- `list`: Collection with CRUD operations and two entry types:
  - **Template node entries**: `list Tasks (entry=<../TaskTemplate>)` - structured objects following a template
  - **Vanilla type entries**: `list Names (entry=string)` - simple primitive values
- `tab`: Tab container for UI organization

#### List Entry Types

Lists support two distinct entry patterns:

**Template Node Lists** - for structured data:
```overseer
// Define a template (usually hidden)
div TaskTemplate (hidden=true) {
    string name = ""
    int priority = 0
    bool completed = false
}

// List using the template
list Tasks (entry=<../TaskTemplate>) {
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
- `chart`: Data visualization element (planned)

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


#### Showing part of a list

A long history costs what it costs to resolve, and most of it is not on screen. `window` says
how many entries to keep, `sort_by` says which ones those are — and the window is applied
before the entries are instantiated, so the rest is work never done rather than work thrown
away.

```overseer
list History (entry=<Day>, key="day", window=3,
              sort_by=$(|x| 0 - millis_since_epoch(x/day))) { }
```

What is left out is still in the file and still saved; it is simply not resolved. A note under
the list says how many entries that was.

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

### 4. Actions and Triggers

Overseer supports interactive elements and automated actions:

#### Interactive Elements:
```overseer
// Button with actions
button save_button "Save Data" {
    action Set (target=../status) = "saved"
    action Add (target=../save_count) = 1
}

// Checkbox with actions
checkbox completed "Mark Complete" {
    action Set (target=../completed) = true
    action Add (target=../../stats/completed_count) = 1
}

```

#### Conditional actions inside action blocks

You can conditionally execute actions inside any `on ... {}` block using an `if` action node. The condition is provided via the `cond` parameter (usually a formula) and, when truthy, the nested actions will run. When falsy, the nested actions are skipped.

Syntax:

```overseer
on click {
    if (cond=$(/* boolean expression here */)) {
        // any number of nested actions
        set (path="../flag", mode="value") = true
        append (list="../Items", template="<Item>") {
            - name = "Created"
        }
    }
}
```

Notes:
- `cond` should evaluate to a boolean. Standard formula comparisons and logical operations are supported.
- Nested content of `if { ... }` must be actions; field assignments inside action blocks follow the usual `- field = value` rules when used within `append`/`prepend` object initializers.
- This conditional is available only within action contexts (e.g., inside `on click {}` or `on timeout {}` blocks).

#### A form that fills an entry

A `textbox` is a field drawn as a box to type into. It takes the cursor with one tap, where a
string is edited with a double one, and `placeholder` says what goes in it while it is empty.

What is typed belongs to whoever is typing, and is never written to the file - the way
`mutable="guarded"` works. The file says what the box starts with, through the usual means, a
value or a formula, and that is all it ever says. A press sends the text along, so actions can
read it; nothing else does.

`from` on `append` or `prepend` fills the new entry from another node instead of naming each
field in the block:

```overseer
div NewTask (layout="horizontal") {
    textbox title (placeholder="what") = ""
    textbox points (placeholder="points") = ""

    button add (label="add") {
        on click {
            append (list="/project/Items", from="..") {
                - added = $(now())
            }
        }
    }
}
```

The entry is still made from the list's template; the source only supplies values. Fields are
matched by name - and by the names of the divs they sit in, with unnamed wrappers ignored on both
sides - and each value is converted to the type the template gives that field: text to a number, a
date or a flag, `3,5` as well as `3.5`. A value that cannot be converted, or an empty one, is left
out, so the template's default stands. A field the block names itself is the block's. Fields the
template does not have are ignored.

The textboxes a copy read are emptied afterwards - back to what they start with - and a textbox
anywhere else keeps what was typed into it.

#### A whole entry as the thing you press

A div can carry `on click` as well as a button can, and then the whole of it is pressed wherever
it is touched - on a phone, a row of a list is a much bigger target than a button at the end of
it.

```overseer
div Item (layout="horizontal") {
    timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
    string handle (hidden=true) = ""
    string name = $(/shopping/Types.filter(|x| x/handle == ../handle)/name)
    float amount = 1

    on click {
        append (list="/shopping/History") {
            - handle = $(handle)
            - amount = $(amount)
        }
        remove (from="/shopping/List", keyField="added", keyValue=$(added))
    }
}
```

The actions run from the div's own place, one level down from where a button inside it would
stand - so a bare `$(amount)` is the entry's own field, where a button would say `$(../amount)`.

Something inside the div that can be changed keeps its own taps, and nothing reaches the div: a
field that can be edited, a checkbox that can be ticked, a button with an `on click` of its own, a
link. Anything that cannot be changed lets the tap through to the div - a worked-out value, a
field declared `mutable=false`, a checkbox that only shows something, a date, a group or a button
with no action of its own. A worked-out value inside a pressable div does not open its formula on
a double tap either, since its taps are the div's. An entry inside another pressable entry takes
the tap itself, and the outer one does not see it.



### Showing a time as a duration

A `timestamp` draws as an absolute moment by default, or as a duration either side of now:

```overseer
timestamp last_done (mode="elapsed") = "2026-09-14T18:00:00+04:00"   // "4 days ago"
timestamp deadline (mode="remaining") = "2026-09-20T09:00:00+04:00"  // "in 2 days"
```

Both redraw once a second without the document changing, which is the point: the value stored
is the moment, and only its reading moves. There is no timer node - there was one, and it also
fired actions on a schedule, which only worked while the app happened to be open. Anything that
has to happen whether or not a document is open belongs outside the document.

### Tags

A `tags` field holds several labels in one string, drawn as chips and matched against a
vocabulary elsewhere in the document. A label not in the vocabulary still shows, marked as no
longer listed — the field really does hold it.

```overseer
div Label (layout="horizontal") {          // the vocabulary: a handle, a name, a colour
    string tag = ""
    string name = ""
    string colour = "#6b7280"
}
list Labels (entry=<Label>, key="tag") { }

tags labels (vocabulary="/project/Labels", width=24%) = "perf, correctness"
```

Handed to a formula, a `tags` field reads as a list, so a document can ask what it holds:

```overseer
bool is_vegan = $(labels.filter(|t| t == "vegan").count() > 0)
```

### Filter

A view over a list, and nothing more: it writes nothing and the document never changes. What
is typed is matched against the fields named in `text` — the fields, not what is on screen,
because what is on screen is formatted and sometimes hidden.

```overseer
filter (target="/project/Items", text="title, commentary", tags="labels",
        status="done", vocabulary="/project/Labels", label="find") { }
```

| parameter | |
| --- | --- |
| `target` | the list it narrows |
| `text` | which fields the typed text is matched against |
| `tags` | the field holding labels, so labels can be picked to narrow by |
| `vocabulary` | where those labels are listed |
| `status` | a field to offer as a third narrowing, by value |

### 3. Layout and Styling

#### Layout Parameters:
```overseer
// Layout direction control
div Container (layout=horizontal, spacing=10) {
    // Children arranged horizontally with 10px gaps
}

div Container (layout=vertical, spacing=5) {
    // Children arranged vertically with 5px gaps
}

div Container (layout=inherit) {
    // Inherits parent's layout direction
}

div Container (layout=opposite) {
    // Uses opposite of parent's layout direction
}

// Margin controls
div Element (margin=10) {
    // 10px margin on all sides
}

div Element (margin-top=5, margin-bottom=15, margin-left=0, margin-right=20) {
    // Individual margin control per side
}
```

#### Visual Styling Parameters:
```overseer
// Background colors
div Container (background-color=#FF0000) {          // Hex colors
div Container (background-color=red) {              // Named colors
div Container (background-color=rgb(0.2, 0.8, 0.5)) { // RGB triplets

// Font styling
text Content (font-size=16px, font-color=blue) {    // Pixels and named colors
text Content (font-size=120%, font-color=#333333) { // Percentages and hex
text Content (font-size=1.2em, font-color=rgb(0.1, 0.1, 0.1)) { // Em units and RGB

// Fixed sizing with multiple units
div FixedBox (width=200px, height=150px) {          // Pixel values
div ResponsiveBox (width=50%, height=auto) {        // Percentage and auto
div FitContent (width=fit-content, height=fit-content) { // Content-based sizing

// Overflow control
div ScrollableBox (width=200px, height=100px, overflow=auto) {
div ClampedBox (overflow-x=hidden, overflow-y=scroll) {

// Border styling
div BorderedBox (border-style=solid 2px black) {    // Solid borders
div BorderedBox (border-style=dashed 1px red) {     // Dashed borders
div BorderedBox (border-style=dotted 3px blue) {    // Dotted borders
div BorderedBox (border-style=none) {               // No border

// Selective border controls
div TableCell (
    border-top=solid 1px gray,
    border-bottom=solid 1px gray,
    border-left=none,
    border-right=solid 2px black
) {

// Corner control
div RoundedBox (border-radius=8px) {                // Rounded corners
div SharpBox (border-radius=0px) {                  // Sharp corners
```

#### Hiding Labels:

A field shows its `label` beside or above its value. A container can say once that the fields
inside it do not, which is what a list drawn as a table wants: the heading names the columns, so
a cell repeating the name says everything twice.

```overseer
list rows (entry=<Row>, view="table", header=true) { }

div Row (layout="horizontal", hide-labels=true) {
    string title (label="Title") = ""      // the label names the column, not the cell
    int points (label="Points") = 0
}
```

It reaches every descendant, so it need not be repeated on groups nested inside, and a container
within that says `hide-labels=false` shows its own fields' labels again - the nearest ancestor to
state anything wins. Like the inherited styling parameters, the copy handed to a descendant is
not written back to the document.

The label is still part of the field, hidden rather than left out. Writing `label=""` instead
takes the name away altogether, which is worth knowing when nothing else on the screen carries
it: outside a table there is no heading, so a hidden label is not shown anywhere.

#### Saying More When Asked:

A field can carry text that is only read when somebody asks for it - the mouse resting on the
field. `hover-text="..."` says it outright; `hover-text=true` says the field's **label** instead,
which needs nothing written twice.

```overseer
float weight (label="Weight", hover-text="Measured before breakfast") = 82.5

list rows (entry=<Row>, view="table", header=true, hover-text=true) { }
```

Like `hide-labels`, it reaches every descendant, so a list says it once and every cell answers
with the name of its own column. That is the pairing worth knowing: a table takes the label out
of each cell because the heading names the column, and far down a long table the heading has
scrolled away - so the cell shows a value and nothing else. Asked with `hover-text=true`, it can
say which column it is in without showing the name on every row.

The same is true of a hidden label anywhere: `hide-labels` takes the naming off the screen, and
this is where the naming can go.

#### Markdown Text Formatting:
```overseer
text MarkdownContent (markdown=true) = "# Heading
**Bold text** and *italic text*

- Bullet points
- `inline code`
- [Links](https://example.com)

```code block```

> Blockquotes"

text PlainContent = "This is plain text without markdown formatting"
```

##### Inline Color Syntax (Markdown Extension)
You can apply color to an inline span of markdown text without assigning a `font-color` parameter to the whole `text` node using the custom angle bracket form:

```
<color=#FF0000 | This text is red>
<color=blue | Named color>
<color=rgb(30,144,255) | RGB value>
```

Rules:
- Pattern: `<color=COLOR | TEXT>`
- Supported COLOR formats: `#RGB`, `#RRGGBB`, named CSS colors (e.g., `red`, `steelblue`), `rgb(...)`, `rgba(...)`, `hsl(...)`, `hsla(...)`.
- The `TEXT` portion is still parsed as markdown after color preprocessing, so you can nest formatting:

```
text Colored (markdown=true) = "Normal <color=#E91E63 | **bold pink** and *italic* inside> outside"
```

Output will render `<span style="color:#E91E63">` wrapping the inner markdown result.
- Unsupported or unsafe color values cause the tag to be stripped, leaving just `TEXT`.
- Does not override or conflict with a node-level `font-color` parameter; spans simply apply inline style precedence.

Security & Sanitization:
- Only whitelisted patterns are allowed; anything else is dropped to prevent script injection via `javascript:` URIs.

Use node-level `font-color` for broad styling, and inline color syntax for emphasis or multi-colored text segments.

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
        div TaskTemplate (background=$(Priority>20?"#FFE6E6":"#E6F3FF")) {
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
    list Data (entry=<../TaskTemplate>, layout=grid, sort=Priority, direction=vertical) {
        div urgent_task {
            Header = "Fix critical bug"
            Description = "System crashes on startup"
            Priority = 25
            Due = "2025-07-25"
        }
        
        div normal_task {
            Header = "Write documentation"
            Description = "Update API documentation"
            Priority = 5
            Due = "2025-07-30"
        }
        
        div completed_task {
            Header = "Setup development environment"
            Priority = 3
            Due = "2025-07-20"
            Completed = true
        }
    }
    
    // Add new task button
    button AddTask "Add New Task" {
        action Create (target=Data, template=<../Task>)
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

### Mounting another document

A `mount` pulls a node from another file in under a name of your own, and from then on it is
addressed like anything else. A catalogue shared between documents that each keep their own
history is what this is for.

```overseer
mount FOODS (source="foods.os/food_catalog/Catalog", lazy=false, mutable=false, hidden=true) { }

// and then, anywhere below:
string name = $(FOODS.filter(|f| f/handle == ../food).map(|f| f/name).first())
```

| parameter | |
| --- | --- |
| `source` | the file, then the path to the node inside it |
| `lazy` | resolve it when something asks rather than on open |
| `mutable` | whether writes through the mount are allowed |
| `hidden` | mount it without drawing it, which is usual for a catalogue |

`source` is relative to the document that declares the mount, not to the working directory.

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

- **Layout**: `layout`, `spacing`, `margin`, `margin-top`, `margin-bottom`, `margin-left`, `margin-right`
- **Styling**: `background-color`, `font-size`, `font-color`, `hide-labels`, `hover-text`, `width`, `height`, `overflow`, `overflow-x`, `overflow-y`
- **Borders**: `border-style`, `border-top`, `border-bottom`, `border-left`, `border-right`, `border-radius`
- **Content**: `markdown`, `hidden`, `entry`, `base`, `placeholder`
- **Actions**: `target`, `condition`, `template`, `active`, `at`, `from`
- **Charts**: `kind`, `data`, `labels`, `title`, `color`, `limit`
- **Lists**: `entry`, `key`, `window`, `sort_by`, `view`, `header`, `sticky`, `lines`
- **Mounts**: `source`, `lazy`, `mutable`
- **Filters**: `target`, `text`, `tags`, `vocabulary`, `status`
- **Tags**: `vocabulary`

---

This specification covers the currently implemented features of the Overseer language parser and runtime engine.
