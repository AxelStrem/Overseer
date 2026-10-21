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
div Tasks (background-color=#F0F0F0, border-style=solid 1px gray) {
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

#### Text that cannot be read

A line that cannot be read - a string without its closing quote, a `{` never closed, a stray word - does not stop the rest of the document from being read. It is passed over and left in the file as it stands, and the page says so in red on the nearest node, with its line: `The text on line 2 has no closing quote: string name = "A page`.

### 2. Data Types

#### Primitive Types:
- `string`: Text data (single line)
- `textbox`: A box to type into - see "A form that fills an entry"
- `text`: Multi-line text with markdown support
- `int`: Integer numbers
- `float`: Floating-point numbers
- `date`: Date values, `"2026-10-18"`
- `timestamp`: A moment, RFC 3339 with its offset, `"2026-10-18T09:30:00+04:00"` - see "Showing
  values"
- `bool`: Boolean true/false, drawn as a checkbox
- `tags`: Several labels out of a list of them, drawn as chips - see "Tags"
- `enum`: One value out of a list of them, drawn as a chip and picked from that list - see "Enum"

#### Numbers

A number is written `12` or `-3`; a point or an exponent makes it a float: `1.5`, `1.`, `.5`,
`2e3`, `1.5E-7`. `inf`, `-inf` and `nan` are floats too, as whole words - like `true`, they are
values, not text, so a string that says `inf` is quoted; `info` is still a word. Formulas read
numbers the same way.

The field's type has the last word: `float x = 1` holds the float 1, and `int x = 1e3` the whole
number 1000. A float that is not whole stays one in an `int` field, as written. A float the engine
writes into a document is written so that it reads back the same: `1e20` and `1e-9` with an
exponent, infinity as `inf`.

#### Container Types:
- `div`: Generic container for grouping and visual placement and styling
- `list`: Collection with CRUD operations and two entry types:
  - **Template node entries**: `list Tasks (entry=<../TaskTemplate>)` - structured objects following a template
  - **Vanilla type entries**: `list Names (entry=string)` - simple primitive values
- `grid`: A fixed number of cells in rows and columns, each made from one template or type - see
  "Grids"
- `tab`: One page of a tab system, anywhere a node can stand - in a div, a list, another tab.
  The tabs that share a parent are one system: a bar of their labels and one page showing at a
  time, drawn where the first of them stands. Two systems at one level are two divs. At the top of
  a document the parent is the document, and the bar is the page-wide one.
  - The parent's `layout` places the bar: above the pages when vertical, beside them when
    horizontal. A system of one tab draws no bar.
  - The header shows the tab's `label`, which may be a formula - a list whose entry template is a
    tab draws one page per entry, each headed by, say, `label=$(date)`.
  - A tab's own `hover-text` is what its header says. A parameter is the tab's alone, so what the
    tab holds says nothing of it.
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
- `chart`: A line, pie or bar chart of what its `plot`s say (see "Charts")
- `image`: A picture kept beside the document (see "Images")
- `mark`: Something drawn with no value of its own, such as a grip (see "Marks")
- `filter`: A view over a list, narrowing what it shows (see "Filter")

#### Other nodes:
- `on <event>`: What happens when the node it is written in is pressed or changed: a block of
  actions (see "Actions and Events")
- `mount`: Another document's node brought in under a name (see "Mounting another document")
- `context`: A name handed down to everything inside a container (see "Contexts")

### 3. Value Assignment

```overseer
type FieldName = value
```

#### Examples:
```overseer
string Header = "My Task"
int Priority = 10
date Created = $(today())
bool Completed = false
enum Status (vocabulary="/Statuses") = "active"
```

A field that states no value has its type's empty one, or its template's (see "Templates and
Inheritance"): an entry of a list writes only what differs from the template it is made from.

### 4. Formulas

Formulas use `$()` syntax and are evaluated at runtime:

```overseer
int DaysUntilDue = $(../Due - today())
string Summary = $("Priority: " + ../Priority + ", Due: " + ../Due)
float Average = $(../Tasks/Data.average(Priority))
```

A formula can stand wherever a value can - a field's value, a parameter, an action's argument -
and is worked out again whenever what it reads changes.

#### Operators:
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparison: `>`, `<`, `>=`, `<=`, `==`, `!=`
- Logic: `&&`, `||`, `!`
- Conditional: `condition ? value_if_true : value_if_false`
- String concatenation: `+` joins two values when either is text that does not read as a number
  (`"the id " + 7` is `"the id 7"`, and an empty field joins as nothing); otherwise it adds them
  (`"5" + "3"` is `8`)

#### Functions:

A moment can be given as a `date`, a `timestamp`, or text holding either; one with no offset is
taken as UTC, and a bare date as its midnight.

| function | |
| --- | --- |
| `today()` | the date today, in the local time zone |
| `now()` | the moment now, a timestamp |
| `days_since(x)`, `minutes_since(x)` | whole days or minutes from `x` to now, negative when `x` is still to come - `minutes_since(due) > 0` is overdue |
| `date_add_days(x, n)` | `n` days after `x`: a date stays a date, a timestamp a timestamp |
| `date_add_hours(x, n)` | `n` hours after `x`, a timestamp; `n` may be fractional |
| `same_day(a, b)` | whether two moments fall on the same local day |
| `weekday(x)`, `day_of_month(x)`, `month_of(x)` | of the local day: 1 for Monday to 7 for Sunday; 1 to 31; 1 to 12 |
| `minutes_of_day(x)` | minutes since local midnight, for a time of day |
| `millis_since_epoch(x)` | the moment as a number, for sorting by it |
| `fresh_key(list, ...)` | a key no entry of the keyed lists given holds - see "A form that fills an entry" |

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

### 6. Entries of a list

An entry of a list is named in one of three ways, and which one depends on who is naming it.

- **By its key, `[key]`.** A list that says `key="handle"` names each entry by what its `handle`
  holds: `project/Items/[parser]/stage`. This is the address the page, the server API
  ([API.md](API.md)) and `scripts/preview.mjs` use, and it stays the entry's whatever is added or
  removed around it. A `/`, `[` or `]` inside a key is escaped with `\`.
- **By its place, `[n]`**, in an action's path: `../Comments[0]` the first entry, `[-1]` the last,
  counting from the end when the number is negative (see "Opening a field for editing").
- **By its template and place, `<Template>__n`.** An entry written without a name is named after
  its list's template and its place, counted from 1: `History/Bought__1`, `Bought__2`. In a list
  without a key this is the entry's address, and it shifts when an entry before it is removed -
  the reason `key=` is worth declaring. Actions and formulas can name an entry this way too.

Two siblings of one name are `name` and `name#1`, in an address as in a path.

A formula does not index a list. It reaches entries through the list's methods: `find` for an
entry by its key, `filter` for those that match, `first` for the first of what is left.

```overseer
int points = $(/project/Items.find("parser")/points)
string first_open = $(Items.filter(|x| x/stage != "done").map(|x| x/title).first(""))
```

Actions that change a list name their entry by a key field and value instead -
`remove (from="/project/Items", keyField="handle", keyValue=$(../handle))` - see "Actions and
Events".

### 7. Lists

A list in a formula is a chain of methods, each taking what the one before it left. A method that
looks at each entry takes a lambda, `|x| ...`, where `x` is the entry and `x/field` one of its
fields; in a list of plain values `x` is the value.

| method | |
| --- | --- |
| `filter(\|x\| condition)` | the entries the condition holds for |
| `map(\|x\| expression)` | what the expression gives for each entry |
| `find(key)` | the entry of a keyed list holding that key |
| `first()`, `first(default)` | the first of them, or the default when there are none |
| `count()` | how many there are |
| `sum()`, `sum(\|x\| x/field)` | their sum, or the sum of what the lambda gives |
| `avg()`, `average()`, and with a lambda | their mean |
| `max()`, `min()` | the largest and smallest - numbers, dates, timestamps or text |
| `reduce(start, \|acc, x\| expression)` | folds them into one value, from `start` |

```overseer
int open = $(/project/Items.filter(|x| x/stage != "done").count())
int points = $(/project/Items.sum(|x| x/points))
float average = $(Readings.avg(|x| x/systolic))
date latest = $(Readings.map(|x| x/day).max())
```

A `tags` field read by a formula is a list of its labels (see "Tags").

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

A window decides what is drawn, not what the list holds. Anything that reaches an entry left out
finds it as it would be in view, with its template and its formulas worked out: a total over the
whole list, a `find` of an old entry by its key, a read of one by its address, a button writing
to one, a div whose `link=` names one by its key. Only what is reached is worked out - a `find` pays for the entry it finds, a sum for every
entry it adds up - and the list still draws its window.

#### Moving entries by hand

`movable=true` lets a list's entries be dragged: to another place in the list, or into another
movable list whose entries are made from the same template, which is how a board of columns is
built - or into a lane of a movable board, which also gives it the lane's value (see "Drawing a
list as a board"). A drag is a write, so it needs writes allowed there (`mutable`). A list drawn in an order
of its own (`sort_by`) takes an entry dropped into it from elsewhere and puts it where its order
says. By hand, its entries are reordered only among those its order ties: those it draws in the
order the file has them, so that is the order a drag writes. The entry moves in one change: it is
never in both lists or in neither, and one undo puts it back.

An entry can say it stays where it is with `movable=false` of its own, usually in its template and
often as a formula, while the rest of its list moves around it:

```overseer
div Item (movable=$(pinned == false)) {
    string title = ""
    bool pinned = false
}
```

Together the two keep pinned entries on top while the rest are dragged around below them:

```overseer
list Items (entry=<Item>, movable=true, sort_by=$(|x| x/pinned == true ? 0 : 1)) { }
```

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
With no column sorted, the list is in the order of the document again. A list with an order of its
own (`sort_by`) keeps it first, and a column sort orders the entries it ties: what it keeps on top
stays on top.

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

The lanes stand side by side as columns. `layout="vertical"` stacks them as rows instead, each
headed by a band in its value's colour, with its entries running across and wrapping - the shape
of a tier list.

A board that also says `movable=true` keeps an order within its lanes, and trades entries with
the movable lists made from the same template. An entry dropped among a lane's entries is moved
there and takes the lane's value, in one change: the field's `on change` runs with it, and one
undo takes all of it back. An entry dragged out of the board into a plain list is only moved,
and keeps its value.

```overseer
list Ranked (entry=<Food>, view="board", by="tier", layout="vertical", movable=true) { }
list Unranked (entry=<Food>, layout="horizontal", movable=true) { }
```

### 8. Grids

A grid holds `columns` x `rows` cells, every one made from its `entry=` - a template, or a field
type such as `int` - without being written. Each cell is named by its place, spreadsheet-style:
column letters `A`..`Z`, `AA`.., and the row from 1, so `C5` is the third column of the fifth row.
What the grid's block writes overrides only the cell it names, matched as an instance's block is
matched onto its template:

```overseer
grid Rota (entry=<Shift>, columns=5, rows=3) {
    - C2 {
        - who = "Bob"
        - hours = 6
    }
}
grid Steps (entry=int, columns=7, rows=2) {
    - A1 = 8200
}
```

- **Only what is written is saved.** A cell set since - by an action, the API or a person in the
  page - is written into the block as `- C5 { ... }` (or `- C5 = 3` for a grid of a type), among
  the others in the order of their places.
- **The size** is a whole number above 0 each, written as it is - not a formula. A cell written for
  a place outside the grid, say after it was made smaller, stays in the file, is not drawn, and is
  said as a problem on the grid.
- **A cell's address** is a step like any other: `week/Rota/C2/who`. An action may also name a
  cell by its place, column then row, each counted from 0 as `[n]` counts a list's entries:
  `set (path="../Rota[2,1]/who")` is `set (path="../Rota/C2/who")`. A grid has no entries to
  `append`, `remove` or `move`.
- **A formula** reaches the cells through the grid's methods, which give a list the list methods
  then work on, row by row:

  | method | |
  | --- | --- |
  | `cells()` | every cell |
  | `row(n)`, `column(n)` | the cells of one row, or one column, from 0 |
  | `at(column, row)` | one cell, from 0 - its value, or `at(...)/field` one of its fields |

  ```overseer
  int hours = $(Rota.cells().sum(|s| s/hours))
  int wednesday = $(Rota.column(2).sum(|s| s/hours))
  int monday = $(Steps.at(0, 0))
  ```

  A place outside the grid is an error, as a field that is not there is.
- **The page** draws the cells in their rows and columns, headed by the column letters and the row
  numbers; `header=false` leaves the headings out.

## Advanced Features

### 1. Templates and Inheritance

A template is an ordinary node, usually kept in a hidden div. An instance is written with the
template's name in angle brackets in its type's place, and a list's `entry=` makes every entry
an instance of it:

```overseer
tab Tasks {
    div (hidden=true) {
        div Task (background-color=$(Priority > 20 ? "#fee2e2" : "#ffffff")) {
            string Header = ""
            int Priority = 0
            date Due = ""
            bool Completed = false
        }
    }

    <Task> pinned {
        - Header = "Read the specification"
    }

    list Data (entry=<Task>) {
        - {
            - Header = "Learn Overseer"
            - Priority = 5
            - Due = "2025-07-30"
        }
    }
}
```

`<Task>` names the template by its name; a path, `<../Task>`, or a file (see "A template from
another file") says where to look when the name alone is not enough. An instance writes only what
differs from its template: `Completed` is `false` in both instances above without saying so.

A template can itself be an instance of another - `<Task> Errand { - Priority = 10 }` in the
hidden div - and what is made from it, an instance or a list's entry, gets everything both give:
the fields, buttons and actions of `Task`, with what `Errand` changed or added on them.

An instance's kind is its template's name, so a template cannot be named after a type - `tags`,
`list`, `button` and the rest under "Reserved Keywords", in any case. A reference to one is
refused, saying why, as a reference to a missing node is: nothing is copied, and the instance is
drawn as a div holding what it was written with.

#### How an instance is matched onto its template

An instance - `<Card> c { ... }`, or an entry of a list with `entry=` - starts as a copy of its
template. An instance written without a name, `<Card> { ... }`, is a wrapper, as an unnamed div
is: it adds no step to any address, and what it holds is named as though it sat in its parent.
What an instance writes is matched onto the copy:

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
- An element that says **`base=none`** starts over: what the template had inside it goes, and its
  own block, if it writes one, is all it holds. `- parts (base=none)` empties `parts` and keeps the
  rest of the instance as the template has it. **`base=<node>`** starts it from that node's block
  instead, found as a template is, and its own block is matched onto that.

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
        div (background-color=#eee) { }
    }
    - owner = "me"
}
```

`subtitle` goes right after `name`; the second tab's second div gets the background; `owner`,
which the template does not have, comes last. Named elements are enough for most instances: the
walk, and the template's unnamed layout it relies on, matter only for reaching an unnamed node or
placing an element among the inherited ones. A list's entries inherit as a div's children do: the
template's come first, and the entries an instance writes in the list's block come after them -
`- Items { }`, or `- Items (label="Mine")` with no block, adds none. The walk does not go into the
entries: an entry the instance writes is a new one, never an edit of one the template gave. The
template's entries are the template's to change: they are not written into the instance's file,
and in the instance they cannot be edited, moved or removed. An entry added in the instance -
appended, prepended, moved or sorted - always stays after them.

Saving gives an instance back as it was written, with only what changed since: a value edited
where the instance wrote it is changed there, and one edited on something only the template gave
becomes an override in the nearest block the instance wrote around it - inside the unnamed step
that reached it (`div (background-color=#eee) { - links = "..." }`), inside a named override
(`- Deep (x=2) { - inner = "..." }`), or on the instance itself, after the element written before
it. A parameter set on a named node the template gave is written as `- Deep (x=3)`.

#### A template from another file

A template reference can start with a file: `<project_base.os/project>` names the node `project`
of `project_base.os`. It works wherever a template goes - an instance's type, a list's `entry=`,
an action's `template=` - and the file is looked for next to the document, or as a path from
there (`<lib/project_base.os/project>`).

```overseer
<project_base.os/project> project {
    - name = "Overseer"
    - Items {
        - {
            - title = "a"
        }
    }
}
```

The base is read as it is written, not worked out on its own: its node is copied in like any
template, so its formulas run on the deriving document's data. A reference inside the base means
what it means there - a bare `<Task>` that names a node of the base names that one, and a file it
names is looked for next to the base. A bare reference the base does not define is left for the
deriving document to supply. A template that holds an instance of itself, in one file or across
several, is refused, as is a reference to a file that is not there or a node it does not have;
the instance is then kept as written. A document is worked out again once a base it was made from
changes, or a file that base was made from, or a missing base appears.

An instance brings the base's node into the document, with all it holds. A template inside that
node is then one of the document's own, named as any is - `<Synth>` or `<patch/Synth>`, inside
the instance or anywhere after it - so a file names the base once, at the top:

```overseer
<brocessor/v200.os/patch> patch {
    - Processors {
        <Synth> {
            - vol = 0.5
        }
    }
}
```

What the base keeps outside the node the instance names is not brought in.

A file that is not next to the document is looked for in libraries: first the user's own, then
the templates that ship with Overseer - its `library` folder - so any document, wherever it is,
can be made from one of them: `<projects/project_template.os/project>`. The user's libraries are
the folders `libraries.txt`, beside `settings.os`, names a line each, looked in in that order; a
line that is blank or starts with `#` is passed over, and a folder not written in full is beside
the file. A program that ships bases of its own adds its folder there, and the user's documents
inherit from them without anything being copied. A file of the same name next to the document is
used first, and one put there later takes over, as one put in a library looked in earlier does;
a change to `libraries.txt` is followed too. A path that climbs out of the folder (`../`) is never
looked for in a library, and nothing a document does writes to one. The server and the desktop
app carry the shipped folder with them; `OVERSEER_LIBRARY` names another.

A document is usually a `.os` file. A program that saves its own files as Overseer text under an
extension of its own, as Brocessor does its `.brot` patches, names that extension in
`extensions.txt`, also beside `settings.os`: one a line, with or without the dot, and lines that
are blank or start with `#` passed over. Such a file then opens, is listed and is saved back under
its own name as any document is. A base or a mount still names a `.os` file.

Unnamed instances need no block: `<project_base.os/tab1>` on three lines gives three tabs. Being
wrappers, they put what they hold at their parent's address, so a `<tags.os/tag_list>` panel in
`project` has its list at `/project/Labels`, and the same field in two of them is `note` and
`note#1`. Named ones keep their addresses apart, `a/note` and `b/note`.

A base's absolute paths (`/project/Items`) assume the name of its root node, so an instance that
uses them keeps that name, or the base uses relative paths (`../Items`).

### 2. Contexts

A context is a name whose value is handed down the hierarchy. `context <name>` declares it for
everything inside the container it is written in, and a declaration nearer down overrides it for
everything inside *that* container:

```overseer
context accent = "#2563eb"
context theme {
    - surface = "#ffffff"
    - text = "#1e293b"
    - line = $(text + "33")
}

tab project (label="Project") {
    div Sidebar {
        context theme {
            - surface = "#16203a"
            - text = "#f1f5f9"
        }
        string title (background-color=$(theme/surface)) = "Inbox"
    }
    string colour = "#e11d48"
    context accent = $(colour)
}
```

- **It is read like any other name**: `$(accent)`, `$(theme/surface)`. A formula finds the nearest
  context of that name above it, as it finds any name, by climbing. A context is not one of the
  things its container holds: a method chain over the container does not count it, and the page
  does not draw it.
- **It covers its whole container**, wherever the line sits among the container's children. A
  context written in an unnamed div or tab belongs to the container around it, as everything else
  written there does. The ones written at the top of a document cover the whole document.
- **An override changes only what it names**, the way an instance changes its template: a named
  element replaces the one it matches, `+` adds one, a value replaces the value, and `base=<node>`
  starts from another node rather than from the context above. An element inside it can start over
  too, as an instance's can (see "How an instance is matched onto its template"): `context theme {
  - backdrop (base=none) }` drops every layer of the backdrop above without naming one, and
  `- backdrop (base=<sunset>)` takes the layers of a node `sunset` - a `context sunset { ... }` at
  the document's top, say - in their place. Two declarations of one name in the same container
  merge in the order they are written.
- **Its formulas are worked out where it is used.** In the example the sidebar's `line` is worked
  out from the sidebar's `text`, though only the outer theme says how. A container that declares a
  context has every context that reads it worked out again there - and every context that reads
  one of those - so a theme that reads `$(mode == "dark" ? ... : ...)` follows a
  `context mode = "dark"` written below it. A context that reads none of them is worked out where
  it was declared, or where something it reads was last declared again: a container that declares
  `context hover-text = true` changes nothing of what `$(accent)` is inside it.
- **Templates and mounts** see the contexts of where they are used: an instance's body, or an entry
  of a list, sees those around the instance; a mounted document sees those of the document that
  mounts it. A context a template declares is declared again in every instance.
- **A name is one thing in its scope.** Any name can be a context, and inside a container that sees
  one the document declared, a node of the same name is refused, saying why.
- **It is saved as written**: the declarations are the document's text, and what a container sees
  is worked out from them each time the document is.
- **It is addressed by its declaration**, once: `project/Sidebar/theme/surface` is the line the
  document wrote, and a write there changes it. What a container sees, worked out, has no address
  of its own, and a node read over the server does not list it among its children.
- **Some names the page reads itself**: `theme` (below), and `vocabulary`, `hide-labels` and
  `hover-text`, which say how the fields inside a container are drawn (see "Tags", "Hiding Labels"
  and "Saying More When Asked"). A field's own parameter of the same name is that field's alone, and
  wins over the context for it.

#### The theme and the mode

Every document is worked out inside the user's own choice of look, which is two contexts kept in a
document of their own, `settings.os` - beside the documents a server serves, and in the desktop
app's settings folder:

```overseer
context theme (base=<themes/default.os/theme>) {
    - accent = "#e11d48"
}
context mode = "system"
```

- **They are seen at the top of every document**, before anything the document declares, so a
  document's own `context theme { - accent = red }` changes one key and keeps the rest, and a part
  of it can change more for what is inside that part. A mounted document sees them through the one
  that mounts it. Where there is no `settings.os` yet, it is as though it held
  `context theme (base=<themes/default.os/theme>) { }` and `context mode = "system"`.
- **A theme is a `context theme`** in a file of its own. Those that ship with Overseer are in its
  library's `themes` folder; one in a `themes` folder beside `settings.os`, or in a user's library,
  is offered beside them, and takes the place of a shipped one of the same name. Its keys are `background`, `surface`,
  `text`, `muted`, `line`, `accent`, `success`, `warning`, `danger`, `font`, `mono`, `radius`,
  `size`, `leading`, `numerals` and `backdrop` (below). `font` is the font of the page, and `mono`
  the one the values of `string` and number fields, code in markdown and the markdown editor are
  drawn in - a monospace font, unless a theme says otherwise: `- mono = $(font)` draws values in
  the page's font. `size` is the size of the text (`14px`), which values and text follow and labels,
  headings, chips and buttons do not - their kinds say theirs; `leading` the height of a line, as
  a factor of the size (`1.5`); `numerals` is `proportional` or `tabular`, digits of one width so
  that figures line up down a column. The shipped theme keeps each mode's colours under `light`
  and `dark` and picks one by `mode`, as `- surface = $(mode == "dark" ? dark/surface :
  light/surface)`, so `context theme { - dark { - surface = "#000" } }` changes the dark colour
  alone and the light one stays as it was. `light` and `dark` are keys of the theme, not kinds of
  element. A theme that leaves a key out leaves that part of the page as it is drawn without one.
- **A theme can bring its own fonts**, so that `font` and `mono` need not name one installed where
  the page is opened. `fonts` holds a key for each family, its name the one `font` and `mono` call
  it by and its value a font file - `.woff2`, `.woff`, `.ttf` or `.otf` - by a path relative to the
  `themes` folders, the one beside `settings.os` looked in first and then the libraries':

  ```overseer
  context theme {
      - fonts {
          - plex_sans = "fonts/plex-sans.woff2"
      }
      - font = "plex_sans, sans-serif"
  }
  ```

  For a theme in its own file the path is beside it, wherever the key is written. One family is
  one file, which a variable font fills with all its weights. A file missing, or outside those
  folders, is left out, and the next family `font` names is drawn instead.
- **A key that holds keys says how one kind of element looks**, and a container overrides one of
  them as it does any key: `context theme { - checkbox { - style = switch } }` draws the `bool` and
  `checkbox` fields inside it as switches rather than boxes, and `style = box` inside that draws
  them as boxes again. The kinds a theme says something about:
  - `checkbox`: `style`, `box` or `switch`.
  - `button`: `hover`, how it answers the pointer over it - `background` (its colour lights up),
    `border`, `glow`, `lift`, `grow` or `none`.
  - `separator`: `style` (`solid`, `dashed`, `dotted`), `thickness` (`1px`), `color` (the theme's
    `line` where it says none) and `length` - `full`, or a size such as `60%` or `12px` for a
    short line in the middle.
  - `field`: `value-frame` - `box`, each value in a padded frame of its own that shows under the
    pointer, or `none`, the value as text; `spacing`, the space above and below a field (`8px`);
    `number-align`, `start` or `end` (the default), where a number sits in its field.
  - `table`: `frame`, `box` or `none` - the frame and padding around the whole table;
    `heading { size, weight, case, tracking, rule }` - the heading row's text size (`11px`), its
    weight (`400` to `700`, `600`), `case` (`as-written` or `upper`), `tracking`, the space
    between letters in em (`0.03`), and `rule`, the line under it (`none`, or a thickness, `1px`);
    `row { padding, rule, pointed }` - the space above and below a row's cells (`8px`), `rule`,
    which draws the rows as lines of the table rather than as cards, with a line of that thickness
    in `line` under each (`none` for none), and `pointed`, `none` or `tint`, shading the row under
    the pointer or, on a touch screen, the one last tapped; `cell-padding`, the space after each
    cell's content, where by default there is `6px` on either side; `detail`, how a `span="row"`
    line is drawn - `frame` as anything else, `bar` (a bar in `accent` down its side, on
    `surface`) or `plain`; and `detail-padding`, the space around that line's content.
  - `chip`: `size` (`11px`), `padding` (`1px 7px`), `label` - the text on a chip coloured by its
    vocabulary, where by default it is light or dark, whichever reads on the colour - and
    `remove` and `add`, the × that takes a tag off and the + of a `tags` field: `always`,
    `pointed` (only on the field, or the table row, under the pointer or last tapped) or `never`.
    The chips of a filter are chips like any.
  - `button`: `height` (as tall as its padding makes it), `padding` (`8px 16px`), `spacing` (the
    space above and below, `8px`), `size` and `weight` of the label (`14px`, `500`), `style` -
    `solid`, filled with its colour; `outline`, a border in `line` and the label in `muted`, both
    in `accent` under the pointer; or `plain`, no border or fill and the label in `accent` - and
    `label`, the label's colour on a solid button, where by default it is whichever of light and
    dark reads on the button. A button the document colours itself keeps its colours whatever
    the `style`.
  - `tab`: `size` (`14px`), `weight` and `padding` (`8px 16px`) of a tab's label.
  - `filter`: `width`, `size` (`12px`) and `padding` (`3px 8px`) of the box a filter is typed in.

  Sizes are written as text, `"14px"`; weights and factors as numbers. A `table`, a `tab` bar and
  a `filter` can also say how the kinds inside them look: `- table { - button { - height =
  "22px" } }` gives the buttons in a table their own size and leaves every other button as it
  was, and `- table { - field { - spacing = "0px" } }` packs its rows. A container inside the
  table that says how buttons look is nearer to them still, and wins.
- **A kind can also have the theme's general keys of its own**:
  `context theme { - button { - accent = "#7c3aed" } - chip { - muted = "#475569" } }` draws the
  buttons violet and the uncoloured chips slate, and leaves everything else in the theme. Each
  kind's own key wins over the general one. That holds inside a part that changes only the general
  `accent`, too, since that part's theme still holds the `button` key from above. A kind's key
  reaches what is drawn inside that element, and another kind inside it starts again from the
  container's theme: a `table` key changes the table's heading and rows, not the fields in it -
  unless it holds that kind's keys, as above. The kinds are `button`, `checkbox`, `chip` (tags and
  enums), `tab` (a tab bar and its tabs), `table` (a list drawn as a table), `field`, `chart`,
  `separator` and `filter`. Each of them changes what its look takes from the theme, and nothing
  it draws in a fixed way: a solid button's label is light or dark to read on its colour whatever
  its `text`, and `button/label` says otherwise.
- **`backdrop` is what is painted over `background`**: layers, each a key of its own, drawn in the
  order written with the first on top, and the `background` colour under them all. A layer has a
  `kind`; one of a kind the page does not draw, `none` among them, is left out.

  ```overseer
  context theme {
      - backdrop {
          - glow {
              - kind = "gradient"
              - from = $(accent + "55")
              - to = "transparent"
              - angle = 135
              - center = "20%"
          }
          - base {
              - kind = "gradient"
              - from = $(background)
              - to = $(surface)
          }
      }
  }
  ```

  A `gradient` goes from `from` to `to` in a straight line along `angle`, in degrees (`180`, top to
  bottom, where it says none). `center` is where along that line the middle of the blend falls
  (`50%`), and `sharpness` how short the blend is: `0%` across the whole length, `100%` a hard edge
  between the two colours. A layer's colours are formulas like any of the theme's, so they follow
  `mode`. Being keys, layers are overridden by name - `- backdrop { - glow { - to = "red" } }`
  changes one and keeps the others, `+` adds one, `- glow { - kind = "none" }` hides one - and
  `- backdrop (base=none)` starts over with none, or with only the layers its own block writes;
  `- backdrop (base=<sunset>)` with the layers of `sunset`, written once and used anywhere. The
  page is painted with the document's backdrop, and a container whose theme makes a backdrop other
  than the one it sits in is painted with its own, on its own background colour, so the one around
  it does not show through. A `button` key can hold a `backdrop` of its own, painted over the
  button's colour in whatever state it is in.
- **`mode` is `"light"`, `"dark"` or `"system"`**, and `"system"` is the scheme the page is in -
  the browser's, or the computer's - sent with every request, and light where nothing says.
- **`theme` is the name the page draws through.** The resolved theme of the document's top is the
  look of the whole page; a container that changes it is drawn in its own, its text in the theme's
  `text` and, where it differs from the one around it, its background in its `background`. Any
  other name is the document's own, `mode` included as a value it can read.
- **They are the document's only until it uses the name for something else.** A field named `mode`
  is that field for itself and what is inside it, where a context the document declares itself
  would be refused: a document that never asked for a theme cannot be told its names are taken.
- **The drawer chooses them**: a theme from those there are, and Light, Dark or System. The mode is
  written to `settings.os` as a value; the theme as the `base=` of its `context theme`, so the keys
  the user changed inside the block stay as they were, now changing the theme chosen.
- **On a server that keeps accounts, each person has their own**: `settings.os` in their folder,
  beside their documents, so each sees every document - another's included - in their own look.
  Nobody signed in sees the server's, at its root.

#### Who is looking

`me` is a context of the environment beside the theme and the mode: `me/handle` is the username of
whoever is looking, and `me/name` what they are called. Both are empty where nobody is signed in,
and in the desktop app.

A document names people by username and shows them by name through the server's directory,
`people.os` at its root: a list `People` keyed by `tag`, one entry per account with its `name`. The
server writes it from its accounts, everyone may read it, and nobody changes it through a document
- a person changes their own name in their profile. Mounted, it is a vocabulary like any other, so
a field holding `ap31` and a `<#ap31>` in any text draw that person's chip with the name they have
now; renamed, they are shown by the new name everywhere, and no document is written for it:

```overseer
mount people (source="../people.os/People", lazy=false, hidden=true, mutable=false) { }

tab notes (mutable=true) {
    context vocabulary = "/people/People"
    enum author (vocabulary="/people/People") = ""
    string by = $(/people/People.find(author)/name)

    button sign {
        on click {
            set (path="../author") = $(me/handle)
        }
    }
}
```

The mount's `source` is relative to the document, as always: `../people.os` from a document in a
person's folder, `../../people.os` from one in a folder of theirs.

### 3. Actions and Events

A document changes itself through what it says happens when something is pressed or changed. An
`on <event>` block, written inside the node it belongs to, holds actions, run one after another
when the event comes:

```overseer
int count = 0
button add (label="+1") {
    on click {
        inc (path="../count")
    }
}
enum stage (vocabulary="/Stages") = "open" {
    on change {
        if (cond=$(../stage == "done")) {
            set (path="../finished") = $(today())
        }
    }
}
date finished = ""
```

#### Events

- `click` - a button pressed, or a div that carries it pressed anywhere (see "A whole entry as the
  thing you press").
- `change` - a field's value changed by hand: typed, picked from a chip, or ticked. The new value is written
  first, and the handler and the value are one change, so the handler reads what was just chosen
  and one undo takes back both.
- `load`, `unload` - on a `mount`; without a block of its own, a mount loads and unloads what it
  names.

#### Actions

A path is found as a formula's is (see "Path References"), from the node that carries the
block - so a button's `../` is the node the button sits in. `path="../node.param"` names a
parameter of the node rather than its value. An entry of a list is named by its place in a path,
`../Comments[-1]/body`, or by a key field and value in the actions that change a list; `keyField`
is the list's own `key` where it names none. A value is written after the action, `= value`, and
can be a formula.

| action | what it does |
| --- | --- |
| `set (path) = value` | writes the value; `mode="formula"` writes a formula as itself, to be worked out where it lands |
| `inc (path, by)`, `dec (path, by)` | adds or takes away `by`, 1 unless it says |
| `toggle (path)` | turns a `bool` over |
| `clear (path)` | removes what the field holds, so its template's or type's value stands |
| `set_now (path, clock)` | today's date, by the `local` clock or `utc` |
| `set_now_ts (path, offset)` | the moment now, `offset` seconds later |
| `append (list, template, value, from) { ... }`, `prepend (...)` | a new entry at the end or the front, made from the list's template (or `template=`), its fields set by the block - `- field = value` - or by `from`, which can also name a list to copy every entry of (see "A form that fills an entry"); a list of plain values takes `value=` |
| `ensure_in_list (list, keyField, keyValue, template, position)` | the entry holding that key, made if there is none - at the end, or the front with `position="first"` |
| `set_in_list (list, keyField, keyValue, field) = value` | writes one field of the entry holding that key |
| `remove (from, keyField, keyValue)` | takes away the entry holding that key; `remove_from_list` and `list=` say the same |
| `move (from, keyField, keyValue, to, at)` | moves the entry to another list, or to place `at` in its own |
| `sort (list, by, order, stable)` | puts the entries in the order `by` gives, `by="$(x/title)"`, `asc` or `desc`, and writes that order |
| `clear_list (path)` | takes away every entry |
| `if (cond) { ... }` | runs the actions inside when the condition holds (see below) |
| `refuse = "why"` | stops the press and writes nothing (see "Saying no to a press") |
| `start_editing (path)` | opens a field for typing (see "Opening a field for editing") |
| `fold (path)`, `unfold (path)`, `toggle_fold (path)` | folds a div away or brings it back (see "Folding a div away") |
| `load_mount (path)`, `unload_mount (path)` | brings in, or lets go of, what a mount names |

A press is one change: every action in it is written together, and one undo takes back all of
it. An action that fails - a path that names nothing, say - refuses the whole press, and nothing of it
is written.

#### Who may change what

`mutable` says what a person may change by hand in the page: `true`, the change is the
document's and is written to it; `false`, it is not offered; `"guarded"`, it is changed for
whoever is looking and never written - which day a tracker shows, what a box holds while it is
typed into. It is inherited, so a tab says it once for everything inside, and it can be a
formula. A document that says nothing is for reading. A drag is a change by hand too, and needs
writes allowed there.

Actions, and programs writing through the server, are not held back by it: the document said what
they do. An action that writes a guarded field changes it for whoever pressed, and the file keeps
what it said.

```overseer
tab Tasks (mutable=true) {
    date showing (mutable="guarded") = $(today())
    int total (mutable=false) = $(Items.count())
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
- This conditional is available only within action contexts (inside `on click {}` or `on change {}` blocks, or another `if`).

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

`from` can name a list, and then every entry of it is copied, in its order, each one as a copy
from that entry would be - `prepend` puts them at the front in the same order. The block applies to
every copy. A list of plain values copies its values. A key the target already holds refuses the
press, as for one entry; to put the copies in place of what a list holds, empty it first, in the
same press:

```overseer
button archive (label="Archive all") {
    on click {
        clear_list (path="/shelf/Archive")
        append (list="/shelf/Archive", from="/shelf/Books") {
            - archived = $(today())
        }
    }
}
```

The source is read as the press has left it so far: a list copied into itself is copied once, and a
list emptied earlier in the press copies nothing.

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

The actions of a press run one after another, and each reads the document as the ones before it
left it. An action that takes away the entry being pressed - a `remove`, or a `move` to another
place - does not stop the ones after it: they go on reading the entry's fields as they were when
it left, so a trash button can remove an entry from every list that might hold it, in any order.
What they write still goes to the document, so writing to a field of the entry taken away refuses
the press: that entry is no longer in it.

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

### 4. Fields and Marks

What a field shows besides its value - icons, chips, pictures, charts - and the nodes drawn with
no value at all.

#### Showing values

A number is shown as it is unless the field says otherwise: `precision` gives the digits after the
point, `format="trim"` drops the zeros that leaves at the end, and `prefix` and `suffix` go either
side of it. What is stored is the number alone.

```overseer
float weight (precision=1, suffix=" kg") = 82.46          // 82.5 kg
float portions (precision=2, format="trim") = 1.5          // 1.5
int price (prefix="$") = 12                                // $12
```

A `timestamp` is stored as a moment and shown in the viewer's own time zone: by `precision`,
`seconds` unless it says `minutes`, `hours` or `days`, or by `format` - `date`, `time` (to the
`precision` given), `datetime` or `iso`.

```overseer
timestamp eaten (format="time", precision="minutes") = "2026-10-18T09:30:00+04:00"   // 09:30
```

`shown` gives the text a field is drawn as in place of its value, on any field that holds one. A
column of counts that is mostly zeros reads best with the zeros left blank:

```overseer
int main (label="Main", shown=$(main == 0 ? "" : main)) = 0
string note (shown=$(note == "" ? "?" : note)) = ""
```

What `shown` gives is the whole text: no `precision`, `format`, `prefix` or `suffix` is added to
it. A number it gives back is drawn as the formula gives it, so a formula that wants a precision
writes it itself. An enum is drawn as the text instead of its chip, and is still pressed to pick.
`shown` is only how the field looks. The value stays what it is in the file, in formulas, in the
filter, when a list is sorted by it and when an entry is copied (`append` with `from=`), and
editing the field opens on the value, not on the text.

`freeze=true` keeps what a formula gave at the moment its entry was made: in a new entry the field
is written with the value its formula works out, and from then on it states that value rather than
following the formula. A day's target that starts as yesterday's, and stays what it was once
yesterday changes, is a formula with `freeze=true`.

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
field holding the key would draw. The key is looked up in the text's own field's `vocabulary`, or
else the nearest `context vocabulary`, so a container that names a list once lets everything inside
mention its entries:

```overseer
tab project (label="Project") {
    context vocabulary = "/project/Items, /project/History"
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
lifts the entry whatever lies around it, and a finger on it drags at once. Where the entry cannot be
dragged - its list is not movable or is sorted with nowhere else to drop into, or the entry says
`movable=false` - it is drawn faint and lifts nothing.

```overseer
div Card (layout="vertical") {
    div (layout="horizontal") {
        mark (type="grip")
        string title = ""
    }
    text notes = ""
}
```

In a list drawn as a table a grip has a column of its own.

`separator` is a line. It runs across the way its parent lays things out - down a horizontal row,
along a vertical column - and its own `layout` turns it the other way. How it is drawn is the
theme's (see "The theme and the mode"): solid, dashed or dotted, how thick, what colour, and whether
it runs the whole way or is a short line in the middle.

```overseer
div Row (layout="horizontal") {
    string name = ""
    mark (type="separator")
    int count = 0
    mark (type="separator", layout="horizontal")
}
```

In a list drawn as a table the rules between columns are separators the entry declares, one for
each place a rule is wanted. Each has a column of its own that takes no room unless it asks for a
`width`, so its line sits on the edge between its neighbours, and the same separator lines up from
row to row and on through the heading. One laid along the row, as the last one above, is a line of
its own across the row, under it or over it as it comes last or first.

A `mark` of a type the page does not know draws nothing.

#### Showing a time as a duration

A `timestamp` draws as an absolute moment by default, or as a duration either side of now:

```overseer
timestamp last_done (mode="elapsed") = "2026-09-14T18:00:00+04:00"   // "4 days ago"
timestamp deadline (mode="remaining") = "2026-09-20T09:00:00+04:00"  // "in 2 days"
```

Both redraw once a second without the document changing, which is the point: the value stored
is the moment, and only its reading moves. There is no timer node - there was one, and it also
fired actions on a schedule, which only worked while the app happened to be open. Anything that
has to happen whether or not a document is open belongs outside the document.

#### Tags

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

A `context vocabulary` (see "Contexts") is the vocabulary of every field inside its container that
names none of its own, and of the chips in their text (see "Chips in text"). A field that names its
own uses that one. So a project names its items once, on its tab, and both the field saying what a
task waits on and every comment that mentions a task draw from it, while its labels and its stage
keep their own lists:

```overseer
tab project (label="Project") {
    context vocabulary = "/project/Items, /project/History"
    ...
}
```

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

#### Enum

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

#### Filter

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

#### Charts

A `chart` draws its `plot`s, one series or one figure each. `kind` says how: `line`, unless it says
`pie` or `bar`.

A line plot reads a list: `source` names it, and `x` and `y` are lambdas giving each entry's point.

```overseer
chart trend (height=320px, width=100%) {
    plot sys (label="Systolic", color="#b42c22", source=$(/blood_pressure/History),
              x=$(|x| x/first_at), y=$(|x| x/avg_systolic))
}
```

`domain-x-min`, `domain-x-max`, `domain-y-min` and `domain-y-max` fix the axes; without them the
axes fit the points. A time on the x axis is drawn as a date.

A pie plot is one slice, its `amount` a number; a bar plot is one bar, its `amount` drawn as a
share of its `limit`, with a line where the limit is, and `suffix` after the figures it shows.

```overseer
chart macros (kind="pie", width=220px, height=150px) {
    plot protein (label="Protein", color="#4A90E2", amount=$(totals/protein * 4))
    plot fat (label="Fat", color="#e2a24a", amount=$(totals/fat * 9))
}
chart allowances (kind="bar", width=240px, height=150px) {
    plot salt (label="Salt", amount=$(totals/salt), limit=6, suffix=" g")
}
```

### 5. Layout and Styling

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

`layout` is `vertical` unless it says `horizontal`, `flow`, `inherit` or `opposite`. `flow` fits as
many equal columns as there is room for and wraps the rest - three cards across on a wide window,
one on a narrow one - with `min-width` saying how narrow a column may get. `alignment` places the
children across the way they run: `near`, `center` or `far`. `shadow` is `soft`, `lifted`,
`floating`, `none`, or a CSS shadow of its own.

```overseer
list Ideas (entry=<Card>, layout="flow", min-width=260px) { }
div Card (layout="horizontal", alignment="center", shadow="soft") { }
```

`view="table"` draws a list as a table, a column for each field of its entries, with `header=true`
for a heading naming them and `sticky=true` to keep the heading in view while the rows scroll. A
node of an entry that says `span="row"` is not a column but a line of its own across the row,
under the cells when it comes last and over them when it comes first - a folded thread of
comments under each task.

```overseer
list Items (entry=<Item>, view="table", header=true, sticky=true) { }
div Item (layout="horizontal") {
    string title = ""
    int points = 1
    div Talk (foldable=true, folded=true, span="row", label="Talk") { }
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
div Thumbnail (width=48, border-radius=0) {         // A size without a unit is in pixels
div (layout=horizontal) {                           // Parts of what is left: the 200px first,
    div Main (width=2fr) {                          // then the rest split two to one, and the
    div Side (width=1fr) {                          // row filled exactly whatever its gaps
    div Logo (width=200px) {

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

A width in parts, `2fr`, is a share of what a horizontal row has left once the widths in pixels
and percent and the gaps between its children are taken: two children at `1fr` and `2fr` split it
one to two, and the row spans its container exactly however many children it has. `auto` is a
share of the rest like `1fr`, but never narrower than what it holds. A table's columns take both
the same way, and a column with no width at all is `1fr`. Parts are shares of a row, so a width in
parts outside one means nothing; there `auto` is the plain width a node would have anyway.

A style is the node's own. `background-color`, `font-color` and `font-size` given to a container
paint that container, and what is inside it is drawn in the theme, as though nothing had been said:
a background shows through what is inside it, but its text keeps the theme's colour and size. A
container whose contents should follow says so by changing the theme for them (see "Contexts"):

```overseer
div Sidebar (background-color="#16203a") {
    context theme {
        - text = "#f1f5f9"
    }
    string title = "Inbox"
}
```

The same holds for the borders, `border-radius`, `shadow` and `font-weight`. They style the node and
what it draws itself - a field's value - and never a node inside it, so `border-style=none` on a
list leaves the frames of the fields in its entries to the theme. On the node itself a parameter
wins over the theme, its kind keys included.

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

A field shows its `label` beside or above its value. `hide-labels=true` on a field hides its own,
and a container can say once that the fields inside it do not, with `context hide-labels = true`
(see "Contexts"). That is what a list drawn as a table wants: the heading names the columns, so a
cell repeating the name says everything twice.

```overseer
list rows (entry=<Row>, view="table", header=true) { }

div Row (layout="horizontal") {
    context hide-labels = true
    string title (label="Title") = ""      // the label names the column, not the cell
    int points (label="Points") = 0
}
```

It reaches everything inside, so it need not be repeated on groups nested within, and a container
within that says `context hide-labels = false` shows its own fields' labels again. A field's own
`hide-labels` wins for that field. Written in an unnamed div, a context is the container's around
it; a group of fields inside a container that keeps its labels says it on each field.

The label is still part of the field, hidden rather than left out. Writing `label=""` instead
takes the name away altogether, which is worth knowing when nothing else on the screen carries
it: outside a table there is no heading, so a hidden label is not shown anywhere.

#### Saying More When Asked:

A field can carry text that is only read when somebody asks for it - the mouse resting on the
field. `hover-text="..."` says it outright; `hover-text=true` says the field's **label** instead,
which needs nothing written twice.

```overseer
float weight (label="Weight", hover-text="Measured before breakfast") = 82.5

div Row (layout="horizontal") {
    context hover-text = true
    ...
}
```

As `context hover-text`, it reaches everything inside its container, so a list's entry template
says it once and every cell answers with the name of its own column, unless a field says its own. That is the pairing worth knowing: a table takes the label out
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

---

## Complete Example

A list of tasks with a vocabulary of stages, a form that adds one, totals worked out from the
list, and buttons that change an entry or take it away:

```overseer
div (hidden=true) {
    div Stage (layout="horizontal") {
        string tag = ""
        string name = ""
        string colour = "#6b7280"
    }
    div Task (layout="horizontal") {
        context hide-labels = true
        string handle (hidden=true) = ""
        string title (label="Task") = ""
        enum stage (label="Stage", vocabulary="/dashboard/Stages", width=12%) = "open"
        int points (label="Points", width=10%) = 1
        date due (label="Due", width=14%) = ""
        int days_left (label="Days left", width=10%) = $(due == "" ? 0 : 0 - days_since(due))

        button done (icon="check", hidden=$(stage == "done")) {
            on click {
                set (path="../stage") = "done"
            }
        }
        button drop (icon="cross") {
            on click {
                remove (from="/dashboard/Tasks", keyValue=$(../handle))
            }
        }
    }
}

tab dashboard (label="Tasks", mutable=true) {
    list Stages (entry=<Stage>, key="tag", hidden=true) {
        - {
            - tag = "open"
            - name = "Open"
            - colour = "#2563eb"
        }
        - {
            - tag = "done"
            - name = "Done"
            - colour = "#10b981"
        }
    }

    div (layout="horizontal") {
        int open (label="Open", mutable=false) = $(Tasks.filter(|x| x/stage != "done").count())
        int points_left (label="Points left", mutable=false) = $(Tasks.filter(|x| x/stage != "done").sum(|x| x/points))
    }

    div NewTask (layout="horizontal") {
        textbox title (placeholder="a new task") = ""
        textbox points (placeholder="points", width=10%) = ""
        textbox handle (hidden=true) = $(fresh_key(/dashboard/Tasks))
        button add (label="Add") {
            on click {
                if (cond=$(../title == "")) {
                    refuse = "say what the task is"
                }
                append (list="/dashboard/Tasks", from="..")
            }
        }
    }

    list Tasks (entry=<Task>, key="handle", view="table", header=true,
               sort_by=$(|x| x/stage == "done" ? 1 : 0)) {
        - {
            - handle = "k7m2"
            - title = "Write the guide"
            - points = 3
            - due = "2026-10-25"
        }
        - {
            - handle = "p4t9"
            - title = "Read the specification"
            - stage = "done"
        }
    }
}
```

---

## File Organization

### Single File

A document is one `.os` file, and everything it needs can be in it. Two ways reach beyond it: a
template from another file (see "A template from another file"), which is how documents share a
structure - the templates that ship with Overseer are in its `library` folder - and a mount, which
is how they share data.

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
| `mutable` | `true` lets writes through the mount, to the mounted file |
| `hidden` | mount it without drawing it, which is usual for a catalogue |

`source` is relative to the document that declares the mount, not to the working directory.

What a mount holds is addressed under the mount's name and then the node `source` names:
`FOODS/Catalog/[apple]/kcal`. A write that lands there - a press, a field edited in the page, an
append over the API - is written to the mounted file, at that node, and the host's own text is
left as it was. Only a mount with `mutable=true` lets it through; on any other the write is
refused, and neither file changes. A new entry of a mounted list is made from the mounted file's
template. Undo in the host takes the write back, unless the mounted file has been written to since,
in which case that file is left as it is.

A lazy mount brings in what it names once something asks for it; `load_mount` brings it in from a
press and `unload_mount` lets it go again (see "Actions and Events").

---

## Comments

A document is meant to be read without them: names, labels and structure say what it holds. A
comment is for what they cannot say - why something is the way it is. Comments are kept where they
are when the document is saved.

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

- **Layout**: `layout` (`vertical`, `horizontal`, `flow`, `inherit`, `opposite`), `spacing`, `margin`, `margin-top`, `margin-bottom`, `margin-left`, `margin-right`, `alignment`, `min-width`
- **Styling**: `background-color`, `font-size`, `font-weight`, `font-color`, `hide-labels`, `hover-text`, `width`, `height`, `overflow`, `overflow-x`, `overflow-y`, `shadow`, `value-padding`
- **Borders**: `border-style`, `border-top`, `border-bottom`, `border-left`, `border-right`, `border-radius`
- **Marks**: `type` (on a `mark`: `grip`, `separator`)
- **Content**: `label`, `markdown`, `hidden`, `mutable` (`true`, `false`, `guarded`), `placeholder`, `kind` (`color`, on a `string` or `textbox`), `foldable`, `folded` (on a `div`), `icon` (on a `button`), `freeze`
- **Values**: `precision`, `format`, `prefix`, `suffix`, `shown`, `mode` (`elapsed`, `remaining`, on a `timestamp`)
- **Actions**: `path`, `list`, `to`, `from`, `template`, `value`, `keyField`, `keyValue`, `field`, `by`, `order`, `at`, `position`, `mode`, `clock`, `offset`, `cond`
- **Charts**: `kind` (`line`, `pie`, `bar`), `domain-x-min`, `domain-x-max`, `domain-y-min`, `domain-y-max`; on a `plot`: `source`, `x`, `y`, `amount`, `limit`, `suffix`, `label`, `color`
- **Lists**: `entry`, `key`, `window`, `sort_by`, `view` (`table`, `board`), `by`, `header`, `sticky`, `movable`, `sortable` (`movable=false` on an entry: it stays where it is), `span` (`row`, on a node of a table's entry)
- **Grids**: `entry`, `columns`, `rows`, `header`
- **Mounts**: `source`, `lazy`, `mutable`, `hidden`
- **Filters**: `target`, `text`, `tags`, `vocabulary`, `status`, `enum`, `hide`
- **Enums**: `vocabulary`, `withhold`, `placeholder`, `shape`
- **Tags**: `vocabulary`, `shape` (on a `tags` or `enum` field, or a `textbox` that picks rather than takes typing)
- **Events**: `on click`, `on change`, `on load`, `on unload`
- **Types** (not to be a template's name): `tab`, `div`, `list`, `grid`, `list_item`, `string`, `textbox`, `text`, `tags`, `enum`, `filter`, `int`, `float`, `date`, `timestamp`, `bool`, `button`, `checkbox`, `chart`, `image`, `mark`, `mount`, `context`
- **Contexts**: `context` (in a type's place), `base` (on a context, and `base=none` or `base=<node>` on an element of an override)

---

This specification covers the currently implemented features of the Overseer language parser and runtime engine.
