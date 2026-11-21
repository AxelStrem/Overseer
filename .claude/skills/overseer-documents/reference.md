# Overseer quick reference

This is a summary of `overseer_syntax_specification.md`. When the two differ, the specification
is right. The section names below are the specification's, for reading further.

## Node types

| type | holds |
| --- | --- |
| `string`, `textbox` | one line of text; a textbox is a box to type into, with `placeholder`, `vocabulary`, `limit`, `kind="secret"` |
| `text` | several lines; `markdown=true` draws it as markdown |
| `int`, `float` | numbers; `inf`, `-inf`, `nan` are floats; `view="progress"` draws a bar up to `limit` |
| `bool`, `checkbox` | true / false, drawn as a checkbox |
| `date`, `timestamp` | `"2026-10-18"`; RFC 3339 with an offset, `"2026-10-18T09:30:00+04:00"` |
| `enum` | one handle from `vocabulary="/path/List"` ("Enum") |
| `tags` | several handles, `"perf, ui"`, from a `vocabulary` ("Tags") |
| `image` | a picture kept beside the document ("Images") |
| `div`, `tab` | containers; unnamed ones are layout only |
| `list` | entries from `entry=<Template>` or `entry=string`/`int`/...; `key=` names the key field ("Lists") |
| `grid`, `array`, `calendar` | fixed cells: `columns`/`rows`; `length`; a cell per day ("Grids", "Arrays", "Calendars") |
| `button`, `chart`, `mark` | pressable; a chart of `plot`s; a grip or separator |
| `mount` | another document's node under a name: `mount FOODS (source="foods.os/catalog/Catalog")` |
| `context` | a name handed down to everything inside a container ("Contexts") |

## Common parameters

- **Content**: `label`, `hidden`, `mutable` (`true`/`false`/`"guarded"`), `placeholder`,
  `hover-text`, `icon`, `foldable`, `folded`, `markdown`.
- **Values**: `precision`, `format="trim"`, `prefix`, `suffix`, `limit`, `color`, `view`.
- **Layout**: `layout` (`vertical`, `horizontal`, `flow`), `spacing`, `margin*`, `alignment`,
  `width`, `height`, `min-width`, `overflow`.
- **Style**: `background-color`, `font-size`, `font-weight`, `font-color`, `border-style`,
  `border-radius`, `shadow`, `hide-labels`.
- **Lists**: `entry`, `key`, `view` (`table`, `board`, `canvas`), `sort_by`, `window`, `parts`,
  `tree`, `movable`, `sortable`, `copyable`, `header`, `sticky`.
- **Top-level nodes**: `tick` (how often `on tick` runs).

Any parameter can be a formula: `background-color=$(late ? "#fee2e2" : "#ffffff")`.

## Formula operators

`+ - * / %`, unary `-`, `> < >= <= == !=`, `&& || !`, `cond ? a : b`. `+` joins text when either
side is text that doesn't read as a number.

## Formula functions

| function | gives |
| --- | --- |
| `today()`, `now()` | the local date; the moment now |
| `days_since(x)`, `minutes_since(x)` | whole days or minutes from `x` to now, negative in the future |
| `date_add_days(x, n)`, `date_add_hours(x, n)` | `x` moved on |
| `same_day(a, b)` | whether two moments share a local day |
| `weekday(x)`, `day_of_month(x)`, `month_of(x)` | 1 (Monday) to 7; 1 to 31; 1 to 12 |
| `day(x)`, `week(x)`, `month(x)`, `year(x)` | sortable text: `"2026-11-03"`, `"2026-W45"`, `"2026-11"`, `"2026"` |
| `minutes_of_day(x)`, `millis_since_epoch(x)` | minutes since midnight; the moment as a number |
| `random(seed, ..., n)` | 0 to n-1, fixed by its seed |
| `fresh_key(list, ...)` | a key that no entry of the given keyed lists holds |
| `depth()` | an entry's depth in a tree list |
| `index()`; `column()`, `row()` | an array element's place; a grid cell's place, from 0 |
| `date()` | a calendar cell's own date |

## List methods (chained, read left to right)

| method | gives |
| --- | --- |
| `filter(\|x\| cond)` | the entries the condition holds for |
| `map(\|x\| expr)` | what the expression gives for each |
| `find(key)` | the entry of a keyed list holding that key |
| `first()`, `first(default)` | the first, or the default when there are none |
| `count()` | how many |
| `sum()`, `sum(\|x\| x/f)`, `avg()`/`average()` | total, mean |
| `max()`, `min()` | of numbers, dates, timestamps or text |
| `reduce(start, \|acc, x\| expr)` | a fold |
| `cells()`, `row(n)`, `column(n)`, `at(c, r)` / `at(n)` / `at(date)` | grids, arrays, calendars |

A path can go on after a method: `Items.find("parser")/points`.

## Events

`on click` (a button, or a div that carries it), `on hold`, `on change` (a field edited by hand),
`on load` / `on unload` (a mount, or a named top-level node when the page opens it), `on tick`
(a named top-level node with `tick=`). `on load` and `on tick` may write only guarded fields.

## Actions

| action | parameters / what it does |
| --- | --- |
| `set (path, mode) = v` | write `v`; `mode="formula"` writes the formula itself |
| `inc`, `dec (path, by)` | add or subtract (1 by default) |
| `toggle (path)` | flip a bool |
| `clear (path)` | drop the value, back to the template's |
| `set_now (path, clock)`, `set_now_ts (path, offset)` | today's date; the moment now |
| `append`, `prepend (list, template, with, value, from) { - f = v }` | a new entry; `value=` for plain lists; `from=` copies fields or a whole list |
| `ensure_in_list (list, keyField, keyValue, template, position)` | the entry with that key, made if missing |
| `set_in_list (list, keyField, keyValue, field) = v` | write one field of a keyed entry |
| `remove (from, keyField, keyValue)` | take an entry away |
| `move (from, keyField, keyValue, to, at)` | to another list, or to place `at` |
| `sort (list, by, order, stable)` | reorder and write the order: `by="$(x/title)"`, `order="desc"` |
| `clear_list (path)` | empty a list |
| `if (cond) { ... }` | run the inner actions when `cond` holds |
| `each (in, as) { ... }` | once per item; `t/done` inside with `as="t"` (default `item`) |
| `press (path)` | run another node's `on click` in the same change |
| `refuse = "why"` | stop the press and write nothing |
| `start_editing (path)` | open a field for typing |
| `fold`, `unfold`, `toggle_fold (path)` | fold a div |
| `narrow (path) = v` | fill or empty a box that narrows a list |
| `call (name, into) { - arg = v }` | ask the host: `open`, `undo`, `copy`, `look`, ... ("Asking the host for an operation") |
| `load_mount`, `unload_mount (path)` | bring a mount in, or let it go |

`keyField` defaults to the list's own `key`. Paths in actions start from the node carrying the
`on` block. `path="../node.param"` writes a parameter.

## Address forms

| form | where |
| --- | --- |
| `tab/div/field` | names of named nodes from the top; unnamed divs/tabs are skipped |
| `List/[key]/field` | an entry of a keyed list, everywhere (page, API, preview, templates) |
| `List/Template__n` | an entry of an unkeyed list, counted from 1, shifting on removal |
| `List[0]`, `List[-1]` | an entry by place, in action paths and preview addresses; never in formulas |
| `name#1` | the second of two siblings with one name |
| `Grid/C5`, `Array/C`, `Calendar/2026-10-20` | a grid cell, an array element, a calendar day |
| `MOUNT/Node/...` | through a mount |
| `<file.os/node>`, `<Catalog/[key]>` | a template in another file; values from a keyed entry |
