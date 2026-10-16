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
- `enum`: One value out of a list of them, drawn as a chip and picked from that list - see "Enum"

#### Container Types:
- `div`: Generic container for grouping and visual placement and styling
- `list`: Collection with CRUD operations and two entry types:
  - **Template node entries**: `list Tasks (entry=<../TaskTemplate>)` - structured objects following a template
  - **Vanilla type entries**: `list Names (entry=string)` - simple primitive values
- `tab`: One page of a tab system, anywhere a node can stand - in a div, a list, another tab.
  The tabs that share a parent are one system: a bar of their labels and one page showing at a
  time, drawn where the first of them stands. Two systems at one level are two divs. At the top of
  a document the parent is the document, and the bar is the page-wide one.
  - The parent's `layout` places the bar: above the pages when vertical, beside them when
    horizontal. A system of one tab draws no bar.
  - The header shows the tab's `label`, which may be a formula - a list whose entry template is a
    tab draws one page per entry, each headed by, say, `label=$(date)`.
  - A tab's own `hover-text` is what its header says, and is not handed down to what the tab holds.
  - Which tab is showing belongs to the viewer: kept for the session, never written to the file.
  - An unnamed tab, like an unnamed div, is no step of any address: putting part of a document on
    `tab (label="Rules") { ... }` changes nothing a formula, an action or the bot refers to.

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
- `image`: A picture kept beside the document (see "Images")
- `mark`: Something drawn with no value of its own, such as a grip (see "Marks")

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
- String concatenation: `+` joins two values when either is text that does not read as a number
  (`"the id " + 7` is `"the id 7"`, and an empty field joins as nothing); otherwise it adds them
  (`"5" + "3"` is `8`)

### 5. Path References

Use filesystem-like paths to reference other nodes. A path starts from the node the formula
belongs to - a field, or for a parameter or an action, the node that carries it:

```overseer
./Data/Header        // this node's own Data, and its Header - and only this node's
../Task              // the parent's Task: `..` is one step up
../../Statistics     // the grandparent's Statistics
Data/Header          // Data looked for on this node, then on each ancestor in turn
/Tasks/Data          // from the nearest ancestor holding a Tasks, else the top of the document
```

`./` and a bare name find the same thing when it is there. The difference is what happens when
it is not: a bare name goes on looking through every ancestor, and `./` stops, so it names this
node's field or nothing. Action paths take the same forms - `set (path="./done")`.

A div with no name is layout and nothing else, and is no step of any path. A field in a row inside
`div Box` counts `..` from Box, exactly as it would outside the row, so grouping fields for the
look of them changes nothing they read. The div's own parameters and actions are its container's
business: `./show` on one reads the container's `show`, and its `..` is the container's parent.

Another document is reached through a mount rather than a path - see "Mounting another document".

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

#### Moving entries by hand

`movable=true` lets a list's entries be dragged: to another place in the list, or into another
movable list whose entries are made from the same template, which is how a board of columns is
built. A drag is a write, so it needs writes allowed there (`mutable`). A list drawn in an order
of its own (`sort_by`) takes an entry dropped into it from elsewhere, but cannot be reordered by
hand. The entry moves in one change: it is never in both lists or in neither, and one undo puts
it back.

```overseer
list Todo (entry=<Card>, movable=true) { }
list Doing (entry=<Card>, movable=true) { }
list Done (entry=<Card>, movable=true) { }
```

An entry is picked up by what lies around its values - the space of the entry and of its fields,
their labels, the gaps between them - never by a value itself, so a value's text can still be
selected. On a phone an entry is picked up by holding it still for a moment, since a finger that
moves at once is scrolling. A grip in the entry (`mark (type="grip")`, see "Marks") picks it up
from anywhere, and at once under a finger.

#### Sorting a table by its columns

`sortable` names the fields a reader may sort a table by. Each of their headings shows a sort
icon, and a click on it turns the column's sort from none to descending, to ascending, and back
to none. The column clicked last decides first, and the ones sorted before it break its ties.
With no column sorted, the list is in the order of the document again.

```overseer
list Items (entry=<Item>, view="table", header=true, movable=true,
            sortable="handle, stage, flags, points") { }
```

A sort is how somebody is looking, like a fold: it lasts as long as the page is open, and
nothing of it reaches the document. An `enum` sorts in the order its vocabulary lists its values,
and `tags` as a number with a bit for each tag it holds, a tag later in the vocabulary taking a
higher bit; numbers, dates and timestamps sort by value and anything else as text. An empty value
goes last whichever way the column sorts. While a reader has a list sorted, its entries cannot be
dragged, even if it is `movable`. Outside a table view `sortable` does nothing.

#### Drawing a list as a board

`view="board"` draws one list in columns, by the value its entries hold in an `enum` field named
by `by`: one column for each value of the field's vocabulary, in the order the vocabulary lists
them, headed by the value's chip and how many entries it shows. A value the vocabulary does not
hold gets a column of its own at the end, so nothing in the file goes unseen.

```overseer
list Tasks (entry=<Task>, view="board", by="stage") { }
```

Dragging an entry into another column writes that value into its field, exactly as picking it
from the field's chip would: the field's `on change` runs with it as one change, `mutable` is
respected, and a value the field `withhold`s takes no drop. Nothing moves in the list itself -
an entry stands in its column where it stands in the list - so a filter, `sort_by` and `window`
work on a board as on any list. Unlike `movable=true`, which moves entries between lists, this
keeps every entry in one list and moves it by what it says.

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

#### How an instance is matched onto its template

An instance - `<Card> c { ... }`, or an entry of a list with `entry=` - starts as a copy of its
template, and what it writes is matched onto that copy:

- A **named** element finds the field of its name, in any order and through unnamed nodes, and
  overrides its value, its parameters and, element by element, its block.
- An **unnamed** element lands on the next unnamed node of the same type (`-` stands for any type),
  walking the template in the order it is written and into unnamed nodes. It never lands on a named
  node. An empty block, `tab { }`, matches a node and keeps all of it.
- A **`+`** in the type's place marks an element new to the instance, put in where the walk has got
  to: `+ subtitle = "s"` (its type inferred), `+int count = 0`, `+div { ... }`. It never combines
  with `-`.
- Every match moves the walk to just after what it matched, so a `+` written after `- name = ...`
  lands right after `name`, and a `+` written first lands before everything.
- An element that matches nothing, written without `+`, is kept after the inherited elements of
  its own block.

```overseer
div (hidden=true) {
    div Card {
        string name = ""
        tab (label="About") {
            string description = ""
        }
        tab (label="Notes") {
            div { string notes = "" }
            div { string links = "" }
        }
    }
}

<Card> card {
    - name = "Overseer"
    +string subtitle = "a language for documents"
    tab { }
    tab {
        div { }
        div (background=#eee) { }
    }
    - owner = "me"
}
```

`subtitle` goes right after `name`; the second tab's second div gets the background; `owner`,
which the template does not have, comes last. Named elements are enough for most instances: the
walk, and the template's unnamed layout it relies on, matter only for reaching an unnamed node or
placing an element among the inherited ones. A list's entries are its data, not its structure: an
instance that writes entries into a list replaces the template's.

Saving gives an instance back as it was written, with only what changed since: a value edited
where the instance wrote it is changed there, and one edited on something only the template gave
becomes an override in the nearest block the instance wrote around it - inside the unnamed step
that reached it (`div (background=#eee) { - links = "..." }`), inside a named override
(`- Deep (x=2) { - inner = "..." }`), or on the instance itself, after the element written before
it. A parameter set on a named node the template gave is written as `- Deep (x=3)`.

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

#### Icons

A button can show an icon, alone or in front of its label: `icon` names one from the library
that comes with Overseer. An icon is drawn in the colour of the text around it.

```overseer
button add (icon="plus", label="Add") { }
button note (icon="note") { }
```

Names are lower case, words joined by `-`. The library holds Overseer's own icons - `cross`,
`check`, `arrow-up`, `arrow-down`, `arrow-left`, `arrow-right`, `ellipsis-h`, `ellipsis-v`,
`burger`, `note` - then every icon of [Lucide](https://lucide.dev/icons) and of
[Tabler](https://tabler.io/icons) under their own names, Tabler's filled ones with `-filled`
added (`heart-filled`). Where two have the same name, the first in that order is the one drawn. A
name the library does not have leaves an empty space the size of an icon. An icon-only button
says its name on hover. A button wider than what it shows - given a `width`, or stretched across
a vertical div - keeps its icon and label together in its middle.

##### Icons in text

Any text a document shows can hold an icon, written `<icon=name>` where the icon goes: a label, a
value, a tab's label, a button's label, a tag or enum name, hover text, markdown.

```overseer
string due (label="<icon=clock> Due") = "today"
button add (label="<icon=plus>") { }
- {
    - tag = "veg"
    - name = "<icon=carrot> vegetable"
}
text note (markdown=true) = "<color=red | <icon=alarm-clock> **late**>"
```

The text is kept as written - in the file, in formulas, in the editor - and only drawing turns
the tag into the icon. `icon="plus"` on a button is the short way of writing a label of
`<icon=plus>`; a button or a chip that shows only icons says its name, or its tag, on hover.
Anything else in angle brackets stays text. Where only text can go - the window's title - the
icons are left out.

A document can bring its own icons: a name with a `/` in it, or ending in `.svg`, is the path of
an SVG file relative to the document's own folder. It is drawn in its own colours. After the name,
`color=` draws any icon - the library's or the document's - in one colour instead of the text's.

```overseer
string carrot (label="<icon=icons/carrot.svg> Carrot") = "fresh"
string late (label="<icon=alarm-clock color=#E91E63> Late") = "2 days"
string pantry (label="<icon=icons/jar.svg color=gray> Pantry") = "full"
```

Only the document's folder and the folders under it can be reached: a path that starts at a root
or a drive, or holds `..`, names no icon, and nor does a file that is not `.svg` or is larger than
256 KB. Such a name, like one the library does not have, leaves an empty space where the icon
would be. The SVG is drawn as a picture, so nothing it carries runs.

##### Chips in text

Any text a document shows can name an entry of a vocabulary (see "Tags"), written `<#key>`: it is
drawn as that entry's chip, with its name, its colour and its `title` on hover - the chip a `tags`
field holding the key would draw. The key is looked up in the nearest `vocabulary`, the text's own
field's or an ancestor's, so a container that names a list once lets everything inside mention
its entries:

```overseer
tab project (vocabulary="/project/Items, /project/History") {
    ...
    text body (markdown=true) = "Waits on <#parser>, as <#escape> did."
}
```

A key no vocabulary holds is drawn as a plain chip of the word. As with icons, the text is kept as
written - in the file, in formulas, in the editor - and a filter matches the key inside it. Inside
markdown code, `` `<#key>` `` stays text, and so does `` `<icon=name>` ``.

Pressing a chip that stands for an entry - in text, in a `tags` field, or an `enum` with nothing to
pick - shows the entry: the folds and tabs around it open, it is scrolled into view and marked for
a moment. An entry the filter hides, or one in a list the page does not draw, is not shown, and
the chip says why on hover.

#### Images

An `image` shows a picture the document keeps beside it: its value is the path of a PNG, JPEG,
WebP or GIF file relative to the document's own folder.

```overseer
image plate (width=50%) = "images/plate.jpg"
image shelf (label="The pantry shelf") = "images/shelf.png"
```

It is drawn as large as it is given room for, keeping its proportions: `width` and `height` size
it, as they size any node. Its label heads it and says what the picture is to whoever cannot see
it; without one, its name does. The same paths are reached as an icon's, under the document's
folder and nowhere else, and a file larger than 8 MB is refused. A path that cannot be reached, or
a file that is not there, leaves an empty space. A picture is fetched only when it is drawn, so one
inside a folded div costs nothing until the div is opened. Markdown text shows the same images,
`![plate](images/plate.jpg)` (see "Markdown Text Formatting").

A picture pasted or dropped into the editor of a markdown text is kept beside the document, in
`images/<document name>/` under a name made from the time, such as
`images/tracker/20261014-153012.png`, and the text gets `![](images/tracker/20261014-153012.png)`
where the caret is. The same types and the same 8 MB hold as for a picture shown. A text that
says `markdown=false`, and a one-line field, take no pictures, since there the reference would
only be words. Removing the reference from the text leaves the file where it is.

#### Marks

A `mark` is drawn and nothing more: it is unnamed, holds no value, and, like an unnamed `div`, is
no step of any address, so it can be added to or taken out of a document without changing a
formula. Its `type` says what it draws.

`grip` is the place an entry of a movable list, or a card of a board, is dragged by: a press on it
lifts the entry whatever lies around it, and a finger on it drags at once. Where the list cannot be
dragged, it is drawn faint and lifts nothing.

```overseer
div Card (layout="vertical") {
    div (layout="horizontal") {
        mark (type="grip")
        string title = ""
    }
    text notes = ""
}
```

A `mark` of a type the page does not know draws nothing.

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

#### Saying no to a press

`refuse` stops a press and says why. Nothing the press did is written - the actions before it
included - and the sentence is what whoever pressed is told: the page shows it beside the button
until something is typed or pressed, and the bot is answered with it. Boxes keep what was typed, so
it can be put right. The sentence can be a formula.

```overseer
button add (label="+ task") {
    on click {
        if (cond=$(../handle == "")) {
            refuse = "say an id"
        }
        if (cond=$(/project/Items.filter(|x| x/handle == ../handle).count() > 0)) {
            refuse = "this id is taken, say another"
        }
        append (list="/project/Items", from="..")
    }
}
```

#### A form that fills an entry

A `textbox` is a field drawn as a box to type into. It takes the cursor with one tap, where a
string is edited with a double one, and `placeholder` says what goes in it while it is empty.
Given a `vocabulary`, it picks tags from that list instead of being typed into - the same chips
and picker as a `tags` field - and what is picked is held the same way as anything typed.

A string or a textbox saying `kind="color"` holds a colour, written `#rrggbb` - the `colour` of
every tag list's entries. The string is drawn with a square of its colour before the code, and a
tap on the square opens the browser's own colour picker; what is picked is written exactly as a
typed code would be, so `mutable` and `on change` hold as they do for typing, and the code can
still be edited with a double tap. A value that is not a colour code draws an empty square. The
textbox becomes the picker itself, and holds what is picked like anything typed.

```overseer
string colour (label="", kind="color", width=30%) = "#6b7280"
```

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
template does not have are ignored, and so is a field the template works out by a formula: it keeps
its formula. A field the source works out is copied as what it works out to.

The source can be a list entry as well as a form, and its lists come along too. A list is matched
by name the way a field is, and each of its entries becomes an entry of the new entry's list of the
same name - made from that list's own template and filled by name in the same way, so a list nested
deeper still comes along as well, in order. This is how a project item closes and reopens: each
handler is one copy between the open list and the closed one, so a field or a list added to both
templates later goes both ways without being named in either.

```overseer
append (list="/project/History", from="..") {
    - finished_at = $(now())
    - status = $(../stage)
}
```

The textboxes a copy read are emptied afterwards - back to what they start with - and a textbox
anywhere else keeps what was typed into it.

A textbox can start from a formula, and then it offers what the formula works out until something
is typed over it - and offers it again, worked out anew, once a copy has emptied it. The project
documents' id box offers a short code for the next item this way:

```overseer
textbox handle (placeholder="id") = $(fresh_key(/project/Items, /project/History))
```

`fresh_key(list, ...)` gives four letters and digits that no entry of the keyed lists it is given
holds as its key, leaving out `0`, `o`, `1` and `l`, which read as one another. It is made from
the keys already used rather than at random: it stays the same while the lists do, rather than
changing each time the document is worked out, and moves on once an entry is added.

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
stand - so `$(./amount)` is the entry's own field, where a button would say `$(../amount)`. A bare
`$(amount)` finds it too, but would go on to an ancestor's `amount` if the entry had none.

Something inside the div that can be changed keeps its own taps, and nothing reaches the div: a
field that can be edited, a checkbox that can be ticked, a button with an `on click` of its own, a
link. Anything that cannot be changed lets the tap through to the div - a worked-out value, a
field declared `mutable=false`, a checkbox that only shows something, a date, a group or a button
with no action of its own. A worked-out value inside a pressable div does not open its formula on
a double tap either, since its taps are the div's. An entry inside another pressable entry takes
the tap itself, and the outer one does not see it.

#### Opening a field for editing

`start_editing` opens a field as though it had been double-tapped, with the cursor in it:

```overseer
div Task (layout="vertical") {
    div (layout="horizontal") {
        string title = ""
        button note (icon="note") {
            on click {
                start_editing (path="../comment")
            }
        }
    }
    string comment (hidden=$(comment == "")) = ""
}
```

It is for a field with nowhere to tap. A comment hidden while it is empty takes no room, so the task
stays one line, and the button beside it opens it. A hidden field is shown for as long as it is
open, and hidden again if it is closed as empty as it was.

The path is found by the same rules as any other action's, and a field that is not there refuses
the press. It changes nothing by itself - the field is written when something is typed into it -
so a press that only opens a field is answered without working the document out again. After
other actions in the same block, the field opens once they have run.

An action's path can name an entry of a list by its place, `[0]` the first and `[-1]` the last,
counting from the end when the number is negative. A list without a key has nothing else to
name an entry by, and this is how a press reaches the entry it has just made - one press adds a
comment and opens it for typing:

```overseer
button add (label="add comment") {
    on click {
        append (list="../Comments") {
            - at = $(now())
        }
        start_editing (path="../Comments[-1]/body")
    }
}
```

A place past either end names nothing, and the press is refused.

#### Folding a div away

A div that says `foldable=true` can be folded away and brought back. With a label, the label is
drawn as a header with a chevron, and pressing it anywhere folds the div or unfolds it: folded,
the header stays and everything under it goes. Without a label there is nothing to press, so the
div folds away whole and only an action brings it back. Either way what is folded is simply not
drawn, and what is around it closes up.

```overseer
div Task (layout="vertical") {
    string title = ""
    div Notes (foldable=true, folded=true, label="Notes") {
        list Comments (entry=<Comment>) { }
    }
}
```

`folded` says where it starts, open unless it is said. A fold is how somebody is looking at the
document, not something the document says: the page keeps it, for that tab, through a redraw
and a reload, and the file, the bot and anyone else looking never see it. It is kept by where
the div is, and inside a list with a `key` by the entry's key, so it stays with its item when the
list is sorted or filtered.

`fold`, `unfold` and `toggle_fold` do the same from any action block, with `path` found as any
other action's is. A target that is not a foldable div refuses the press. Like `start_editing`
they change nothing in the document, so a press that only folds is answered without working it
out again. A button whose press opens a div - `toggle_fold` or `unfold` - is drawn pressed while
the div is open, so a row of such buttons says which of its sections are showing.

```overseer
button notes (label="notes") {
    on click {
        toggle_fold (path="../Notes")
    }
}
```

`folded` can be a formula, and that is how one press folds many divs. A fold made by hand holds
only while the document goes on saying what it said when the fold was made; once `folded` reads
otherwise, the div follows the document again. So a guarded flag, turned over by a button, folds
or opens every div that reads it, and each can still be opened by hand afterwards:

```overseer
bool fold_all (mutable="guarded") = false

button all (label="fold all") {
    on click {
        toggle (path="/project/fold_all")
    }
}

div Notes (foldable=true, folded=$(/project/fold_all), label="Notes") { ... }
```

`hidden=true` wins over any fold.



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

An entry of the vocabulary is known by its list's `key` - `tag` when the list names none - so a
list of tasks keyed by `handle` serves as well as a tag list. Its `title`, where it has one, is
what its chip says on hover. Several lists can be named, separated by commas: the picker offers
the first, never the entry the field is part of, and the rest only name and colour what is
already held. A task naming the tasks it waits on, open or finished, each in its own colour:

```overseer
tags after (vocabulary="/project/Items, /project/History") = "parser, escape"
```

A `vocabulary` is handed down, as `hide-labels` is: said on a container, it is the vocabulary of
every field inside that names none of its own, and of the chips in their text (see "Chips in
text"). A field that names its own uses that one. So a project names its items once, on its tab,
and both the field saying what a task waits on and every comment that mentions a task draw from
it, while its labels and its stage keep their own lists.

`shape` says how its chips are drawn: `pill`, rounded at the ends, which tags are unless told
otherwise, or `box`, a rectangle with softened corners, which an `enum` is. A filter's row of
chips takes the shape of the field it narrows by.

```overseer
tags labels (vocabulary="/project/Labels", shape="box") = "perf"
```

Handed to a formula, a `tags` field reads as a list, so a document can ask what it holds:

```overseer
bool is_vegan = $(labels.filter(|t| t == "vegan").count() > 0)
```

### Enum

One value out of a list of them: a task's stage, say. The same vocabulary a `tags` field reads -
a handle, a name, a colour - and the same chip, holding exactly one. Pressing it offers the other
values in the order the list gives them, and the one picked replaces it. A value the list does not
hold still shows, marked, as an unlisted tag does; with nothing chosen, a dashed chip is the place
to press, showing the field's `placeholder` if it has one - stated empty, `placeholder=""`, the
place says nothing until the mouse is on it. The picker never offers the entry the field is part
of. Its chip is a box rather than a
tag's pill, so the one value of a list reads apart from a set of labels beside it; `shape="pill"`
draws it as a tag.

```overseer
div Stage (layout="horizontal") {
    string tag = ""
    string name = ""
    string colour = "#6b7280"
}
list Stages (entry=<Stage>, key="tag") { }

enum stage (vocabulary="/project/Stages") = "filed"
```

Stored as the plain text of the handle, so a formula compares it as the string it is:
`Items.filter(|x| x/stage == "ready").count()`.

Like any field it can say what happens when it changes, with `on change`. The value is sent with
the handler and the two are written as one change, so the handler reads the value just chosen and
one Undo takes back both - so an `if` there can act on it:

```overseer
enum stage (vocabulary="/project/Stages") = "filed" {
    on change {
        if (cond=$(../stage == "finished")) {
            append (list="/project/History") {
                - handle = $(../handle)
            }
            remove (from="/project/Items", keyField="handle", keyValue=$(../handle))
        }
    }
}
```

`withhold` names values not to offer just now, separated by commas - usually a formula, as in
`withhold=$(open_kids > 0 ? "finished" : "")`, which keeps a task with work still open under it
from being finished. It changes only what the picker offers: a value held back can still be
written some other way, so a handler that must not act on it says so in its own `if`.

### Filter

A view over a list, and nothing more: it writes nothing and the document never changes. What
is typed is matched against the fields named in `text` — the fields, not what is on screen,
because what is on screen is formatted and sometimes hidden.

```overseer
filter (target="/project/Items", text="title, handle, Comments", enum="stage", tags="labels, flags",
        status="done", vocabulary="/project/Labels", label="find") { }
```

| parameter | |
| --- | --- |
| `target` | the list it narrows |
| `text` | which fields the typed text is matched against; a name that is a list or a div searches every field under it, so naming a list of comments finds an entry by anything said in them |
| `tags` | the fields holding labels, comma-separated: each gets a row of chips to narrow by, and picking two - in one row or across them - finds the entries holding both |
| `vocabulary` | where the first field's labels are listed; the others are drawn from their own fields' vocabularies |
| `status` | a field to offer as a third narrowing, by value |
| `enum` | an `enum` field, its values offered as chips from its own vocabulary; picking two finds either |
| `hide` | a field to offer one box for, which leaves out the entries where it is above nought |

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
string title (font-weight=$(../kids > 0 ? "bold" : "normal")) = ""  // bold, normal, or 100-900

// Fixed sizing with multiple units
div FixedBox (width=200px, height=150px) {          // Pixel values
div ResponsiveBox (width=50%, height=auto) {        // Percentage and auto
div FitContent (width=fit-content, height=fit-content) { // Content-based sizing

// Overflow control
div ScrollableBox (width=200px, height=100px, overflow=auto) {
div ClampedBox (overflow-x=hidden, overflow-y=scroll) {
div (layout=horizontal) {                           // A container that scrolls takes the height
    list Exercises (width=50%, overflow-y=auto) {   // its row gives it: here the chart's, not
    chart Trend (height=800px, width=40%) {         // the list's own

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

A width in pixels is kept. In a horizontal row too narrow for it, the row wraps it to the next line rather
than squeezing it, and a row wider than the page scrolls sideways inside a container that says
`overflow-x=auto`. A width in percent is a share of the row and shrinks with it. Together they lay out
columns of fixed-width cards that scroll rather than squash:

```overseer
div (overflow-x=auto) {
    div (layout="horizontal", width=1500px) {
        list Ideas (entry=<Card>, layout="horizontal", width=820px) { }
        list Done (entry=<Card>, layout="horizontal", width=420px) { }
    }
}
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

A fenced block - between lines of three backticks - is drawn as a box of code, coloured as the
language named after the opening fence (```` ```rust ````, ```` ```python ````), or as the one it
looks most like when none is named. ```` ```text ```` keeps it plain, as does a language the page
does not know.

Markdown is the only markup a text field has. HTML written in one is shown as the text it is, never
drawn - a field may hold what an agent quoted from a page, and the page holds the server's token. A
link or an image goes only to an `http`, `https` or `mailto` address or to a path on the document's
own server; one to anything else is drawn as its words.

An image whose path has no scheme and no leading `/`, `![plate](images/plate.jpg)`, is one beside
the document, found and refused exactly as an `image` node's is (see "Images"). An image from an
`http` or `https` address is drawn as it is, and is not told which page asked for it.

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
- `TEXT` can hold icons, `<color=red | <icon=clock> late>` (see "Icons in text").
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
- **Styling**: `background-color`, `font-size`, `font-weight`, `font-color`, `hide-labels`, `hover-text`, `width`, `height`, `overflow`, `overflow-x`, `overflow-y`
- **Borders**: `border-style`, `border-top`, `border-bottom`, `border-left`, `border-right`, `border-radius`
- **Marks**: `type` (on a `mark`: `grip`)
- **Content**: `markdown`, `hidden`, `entry`, `base`, `placeholder`, `kind` (`color`, on a `string` or `textbox`), `foldable`, `folded` (on a `div`)
- **Actions**: `target`, `condition`, `template`, `active`, `at`, `from`
- **Charts**: `kind`, `data`, `labels`, `title`, `color`, `limit`
- **Lists**: `entry`, `key`, `window`, `sort_by`, `view` (`table`, `board`), `by`, `header`, `sticky`, `lines`, `movable`, `sortable`
- **Mounts**: `source`, `lazy`, `mutable`
- **Filters**: `target`, `text`, `tags`, `vocabulary`, `status`, `enum`, `hide`
- **Enums**: `vocabulary`, `withhold`, `placeholder`, `shape`
- **Tags**: `vocabulary`, `shape` (on a `tags` or `enum` field, or a `textbox` that picks rather than takes typing)

---

This specification covers the currently implemented features of the Overseer language parser and runtime engine.
