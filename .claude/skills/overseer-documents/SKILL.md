---
name: overseer-documents
description: Reading, writing, changing or checking Overseer documents - `.os` files and the templates, formulas, lists and actions in them - and reading or writing them through the Overseer server API. Use whenever a task opens, edits or designs a .os file, a template from Overseer's library, or a project tracker kept as one.
---

# Overseer documents

An Overseer document (`.os`) is plain text: a tree of nodes that is the data, the page's layout
and the logic all at once. This skill is a head start. The full language is in
`overseer_syntax_specification.md` in the Overseer repository, and when this skill and the
specification disagree, the specification is right. `reference.md` beside this file lists every
function, method, action and address form in one place.

## The shape of a node

```overseer
type name (param=value, ...) = value { children }
```

- **Fields** hold a value: `string`, `text` (several lines; markdown with `markdown=true`),
  `textbox`, `int`, `float`, `bool`, `date` (`"2026-10-18"`), `timestamp`
  (`"2026-10-18T09:30:00+04:00"`), `enum` (one handle out of a vocabulary), `tags` (several,
  comma-separated in one string), `image`. `label=` is what the
  page shows beside the value.
- **Containers**: `div`, and `tab` (one page of a tab bar). **A div or tab without a name is
  layout only**: it is no step of any address. Wrap fields in unnamed divs
  (`layout="horizontal"`) or tabs to change how they look, and no formula breaks.
- **Lists** hold entries made from a template, `list Books (entry=<Book>, key="title")`, or plain
  values, `list Names (entry=string) { - "a" }`. `grid`, `array` and `calendar` hold fixed cells.
- **Interactive**: `button`, `checkbox`, `chart`, `mark`. Behaviour lives in `on click`,
  `on change`, `on load`, `on tick` blocks inside the node.
- `-` in the type's place means "the type the template says". In an entry you write
  `- field = value`.
- Comments are `//` and `/* */`. Use almost none: names, labels and structure should say what a
  document holds, and a comment is only for *why*.

## Templates and defaults

A template is an ordinary node, usually inside `div (hidden=true) { ... }`. An instance is
`<Book> dune { - title = "Dune" }`, and every entry of a list with `entry=<Book>` is one. **An
instance writes only what differs from its template.** A field it leaves out has the template's
value, so never restate a default, and never copy a structure twice: make it a template.

- Matching: a named element finds the field of that name, anywhere in the template, and overrides
  it. A `+` marks a new element (`+int extra = 0`). `base=none` empties an inherited element.
- Stacking: `<Card, Fancy> c { }` is `Card` with `Fancy` merged on top. In a list, `- <Fancy> { }`
  stacks onto the list's template, and `- <Catalog/[apple]> { }` takes the values that entry of a
  keyed list writes.
- From another file: `<projects/project_template.os/project> project { ... }`. The file is looked
  for beside the document, then in the user's libraries, then in Overseer's `library/` folder.
- A template can't be named after a type (`list`, `tags`, `button`...).

## Addresses and paths

Paths read like a file system's, starting from the node the formula or action belongs to (for a
button's action, the button):

| path | finds |
| --- | --- |
| `../read` | a sibling: the parent's `read` |
| `./read` | this node's own `read`, or nothing |
| `read`, `Data/read` | looked for on this node, then on each ancestor in turn |
| `/reading/Books` | from the nearest ancestor holding `reading`, else the top of the document |
| `../node.param` | (in actions) a parameter of the node, not its value |

Entries of a list:

- **Keyed list** (`key="handle"`): `project/Items/[parser]/stage`. It is stable whatever is
  added or removed around it. Escape `/`, `[`, `]` in a key with `\`.
- **Unkeyed list**: `History/Bought__1`, the template's name and the entry's place, **counted from
  1**. It shifts when an entry before it is removed, which is why lists should declare `key=`.
- **In an action's path**, `[0]` is the first entry and `[-1]` the last: `../Comments[-1]/body`.
- Two siblings with one name are `name` and `name#1`.

## Formulas

`$(...)` can stand wherever a value can: a field's value, a parameter, an action's argument. It is
worked out again whenever what it reads changes.

```overseer
int left = $(pages - read)
string shown = $(done ? "done" : "open")
int open = $(/project/Items.filter(|x| x/stage != "done").count())
int points = $(/project/Items.find("parser")/points)
string first = $(Items.map(|x| x/title).first(""))
```

- Lists are read through methods only: `filter`, `map`, `find(key)`, `first(default)`, `count`,
  `sum`, `avg`, `max`, `min`, `reduce`. In a lambda, `x/field` is one of the entry's fields.
- `+` joins text when either side is text that doesn't read as a number, and otherwise adds.
- Dates: `today()`, `now()`, `days_since`, `date_add_days`, `weekday`, `day()`/`week()`/`month()`
  and more (see `reference.md`). An `enum` or `tags` value compares as its handle text.

## Actions and who may change what

```overseer
button finish (label="Finished") {
    on click {
        if (cond=$(../read == ../pages)) {
            refuse = "already finished"
        }
        set (path="../read") = $(../pages)
        append (list="/reading/Log") {
            - title = $(../title)
        }
    }
}
```

- The actions are `set`, `inc`/`dec`, `toggle`, `clear`, `set_now`, `set_now_ts`,
  `append`/`prepend`, `ensure_in_list`, `set_in_list`, `remove`, `move`, `sort`, `clear_list`,
  `if`, `each`, `press`, `refuse`, `start_editing`, `fold`, `narrow`, `call`, `load_mount`.
  Their parameters are in `reference.md`.
- A press is **one change**, so one undo takes all of it back. If any action fails (a path that
  names nothing, a `refuse`), nothing of the press is written.
- `set` on a field does not run that field's `on change`. `press (path=...)` runs another node's
  `on click`.
- **`mutable`** is what a *person* may change by hand: `true`, `false`, or `"guarded"`
  (changed for the viewer, never written to the file). It is inherited, and **a document that says
  nothing is read-only in the page**, so the tab or div that people edit says `mutable=true`.
  Actions and the server API are not held back by it. `on load` and `on tick` may write only
  guarded fields.

## A small document, as it should look

```overseer
div (hidden=true) {
    div Book (layout="horizontal") {
        string title (label="Title") = ""
        int pages (label="Pages") = 0
        int read (label="Read") = 0
        int left (label="Left", mutable=false) = $(pages - read)

        button finish (label="Finished") {
            on click {
                set (path="../read") = $(../pages)
            }
        }
    }
}

tab reading (label="Reading", mutable=true) {
    int pages_left (label="Pages left", mutable=false) = $(Books.sum(|x| x/left))

    list Books (entry=<Book>, key="title", layout="vertical") {
        - {
            - title = "Dune"
            - pages = 412
            - read = 120
        }
    }

    button add (label="Add a book") {
        on click {
            append (list="../Books") {
                - title = $("Book " + (../Books.count() + 1))
            }
        }
    }
}
```

## Mistakes agents make

- **Indexing in a formula.** `Items[0]`, `Items[1]/points` and `.map(...)[1]` are errors. Use
  `find("key")`, `filter(...).first()`, or `at(n)` on a grid or array. `[n]` works only in an
  action's path, and in the addresses `scripts/preview.mjs` takes.
- **Methods that don't exist**: `where`, `sort`, `sort_decreasing`, `find_max`, `find_min`,
  `length`, `size`. Use `filter`, `count`, `max`, `min`, and the `sort` *action* to reorder a list.
- **Building unkeyed addresses by hand.** `Bought__n` counts from 1 and shifts. Read the list
  first (over the API, the answer's `child_addresses`), or give the list a `key=`.
- **Restating defaults** in every entry, or repeating a block of fields instead of making a
  template.
- **Naming layout divs.** A name adds a step to every address under it, so name a node only when
  something refers to it.
- **Forgetting `mutable=true`.** Without it the page lets nobody edit anything.
- **Quotes in strings.** Strings are double-quoted. Inside them, `\"`, `\\`, `\n` and `\t` are the
  escapes. A `text` value can also run over several lines inside its quotes.
- **Absolute paths in a base template.** `/project/Items` assumes the root keeps that name, so an
  instance of a library template keeps the template's root name, or the template uses `../`.
- **Reformatting.** A save gives back the bytes it was given. Change only the lines you mean to,
  and keep the file's indentation (four spaces) and line endings.
- **Writing to someone's live documents to try something.** Work on a copy. Never run a server
  that writes against a real documents folder.

## Checking a document

In the Overseer repository, `scripts/preview.mjs` serves a *copy* of a document, opens it in
headless Chrome or Edge, saves screenshots and prints what failed: templates or mounts that could
not be found, formulas that failed, errors the page threw. It needs the page and the release
server built first:

```bash
npm run build
cd src-tauri && cargo build --release --features server --bin overseer-server && cd ..
node scripts/preview.mjs path/to/doc.os                     # screenshot + problems
node scripts/preview.mjs path/to/doc.os --mobile            # at phone width
node scripts/preview.mjs path/to/doc.os --script steps.mjs  # scripted presses on the copy
```

A script is the body of an async function. It can call `press('reading/Books/[Dune]/finish')`,
`type(text)`, `pick(address, option)`, `tab(name)`, `text(address)`, `shot(name, { of })`,
`problems()`, and `changes()` (what the file gained and lost). `scripts/preview_example.mjs` is an
example, and `node scripts/preview.mjs --help` lists every helper. **A document is done when it
draws as intended, prints `problems: none`, and its buttons write what you meant.** Look at the
screenshots: they show layout mistakes that no error reports.

## Over the server

`API.md` describes the HTTP API: `GET /v1/node` and `/v1/source` to read, `POST /v1/node` to set
a value, `/v1/append` and `/v1/remove` for list entries, `/v1/event` to press a button, and
`/v1/undo`. A document and an address go in the query string. Values travel typed:
`{"String": "milk"}`, `{"Integer": 2}`, `{"Float": 2.5}`, `{"Boolean": true}`,
`{"Date": "2026-10-18"}`, `{"Timestamp": "2026-10-18T12:00:00Z"}`. A bare `2` is refused.
Pressing a button through the API runs its actions, the document's own rules included, so that
is usually better than writing the fields yourself.

## Where to look next

- `overseer_syntax_specification.md`: every node type, parameter, function and action.
- `AGENTS.md`: the short guide in the repository root.
- `examples/` (working documents; `examples/basic` has one small document per feature) and
  `library/` (the templates and themes any document can be made from). Before writing something
  new, find a document that already does something like it and follow its style.
