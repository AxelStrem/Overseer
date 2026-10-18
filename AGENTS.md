# Overseer, for agents

A short guide to writing and checking Overseer documents. The language in full is in
[overseer_syntax_specification.md](overseer_syntax_specification.md); this is the part you need
first.

## What a document is

A `.os` file is a tree of nodes, and the tree is the data, the layout and the logic at once. A
node is a type, an optional name, optional parameters, and a value or a block:

```overseer
type name (param=value, ...) = value { children }
```

- **Fields** hold a value: `string`, `text` (markdown), `int`, `float`, `bool`, `date`,
  `timestamp`, `enum` (one value out of a list), `tags` (several), `image`. `label=` is what the
  page shows beside the value; a field without one shows the value alone.
- **Containers** group: `div`, and `tab` for a page of a tab bar. A div or tab **without a name is
  layout only** - it is no step of any address, so fields can be regrouped, put in rows
  (`layout="horizontal"`) or on tabs without changing a formula that reads them.
- **Lists** hold entries made from a template: `list Books (entry=<Book>, key="title")`. The
  template is an ordinary node, usually in a hidden div. Each entry, `- { ... }`, writes only the
  fields that differ from the template - anything it leaves out has the template's value. `key=`
  names the field an entry is known by.
- **Formulas**, `$(...)`, stand wherever a value can: `$(pages - read)`,
  `$(Books.filter(|x| x/left > 0).count())`. Paths are like a file system's: `../read` is a
  sibling of the node the formula belongs to, `/reading/Books` starts from `reading` wherever the
  formula is, and a bare name is looked for on the node and then up through its ancestors. Lists
  are read through
  methods - `filter`, `map`, `find`, `sum`, `count`, `first` and the rest.
- **Actions** run on events: `on click` in a button (or a div), `on change` in a field. `set`,
  `inc`, `append`, `remove`, `move`, `if`, `refuse` and the rest change the document; a press is
  written as one change, and one undo takes it back.
- **`mutable`** says what a person may change by hand: `true`, `false`, or `"guarded"` (changed
  for the viewer, never written). It is inherited; a document that says nothing is read-only in the
  page. Actions are not held back by it.

```overseer
div (hidden=true) {
    div Book (layout="horizontal") {
        string title (label="Title") = ""
        int pages (label="Pages") = 0
        int read (label="Read") = 0
        int left (label="Left") = $(pages - read)

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
                - title = "Untitled"
            }
        }
    }
}
```

An entry of a keyed list has the address `reading/Books/[Dune]/read`. An entry of a list without a
key is `<Template>__n` by its place, counted from 1, and shifts when one before it goes. The
specification's "Entries of a list" says which form each tool takes.

## Style

- **A document explains itself.** Names, labels and structure say what it holds; write minimal to
  no comments, and only for why, never for what.
- **Templates, not repetition.** Anything that appears twice is a template, and its instances
  write only what differs. Defaults are left out, not restated.
- **Unnamed containers for layout, named ones for meaning.** A name is part of every address
  under it, so name a node because something refers to it, and wrap fields in unnamed divs and
  tabs for how they look.
- **Prefer what the language has.** Before adding to the engine for one document's sake, look
  for a combination of existing features; a new feature should be general, with uses beyond the
  case at hand.
- A save gives back the bytes it was given. A document that reformats itself when saved is a bug
  in the writer.

## Reading further

- [overseer_syntax_specification.md](overseer_syntax_specification.md) - every node type,
  parameter, function and action.
- [`examples/`](examples) - working documents: trackers, tasks, shopping, projects; `examples/basic`
  holds one small document per feature.
- [`library/`](library) - the templates and themes that ship with Overseer, which any document can
  be made from (`<projects/project_template.os/project>`).
- [API.md](API.md) - reading and writing documents over the server: addresses, values, appending,
  pressing buttons, undo.

## Seeing a document

`scripts/preview.mjs` serves a copy of a document, opens it in headless Chrome or Edge, and saves
screenshots: what a person would see, without touching the real file. It needs the page and the
release server built first:

```bash
npm run build
cd src-tauri && cargo build --release --features server --bin overseer-server && cd ..
node scripts/preview.mjs examples/tasks/tasks.os            # one screenshot, and what went wrong
node scripts/preview.mjs path/to/doc.os --script steps.mjs  # scripted presses against the copy
```

A script presses, types and picks as a person would - `press('reading/Books/[Dune]/finish')`,
`type(...)`, `pick(address, option)` - reads back `text(address)`, and ends with `changes()`,
what the file gained and lost. [scripts/preview_example.mjs](scripts/preview_example.mjs) is one;
`node scripts/preview.mjs --help` lists every helper. Screenshots go to `--out`, else to
`AGENT_SCRATCH`.

Check a document you wrote this way before calling it done: that it draws, that its formulas
work (`problems: none`), and that its buttons write what you meant.
