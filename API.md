# The server API

`overseer-server` serves a folder of documents over HTTP: the page reads and writes them through
it, and so can anything else - the companion bot does. This is the part meant for programs: list
the documents, read the node at an address, set a value, append or remove a list entry, press a
button the document declares, and take the last write back.

## Starting it

```bash
cd src-tauri
cargo run --release --features server --bin overseer-server -- --root ../examples --frontend ../dist
```

`--root` is the documents folder (default `../examples`), `--frontend` the built page (default
`../dist`; without it the API still works), `--port` the port (default `4747`), `--bind` the
address (default `127.0.0.1`). The token comes from `OVERSEER_TOKEN` or `--token-file PATH`, never
from the command line. Without a token the server answers anyone on this machine; bound anywhere
but loopback, it refuses to start without one unless `--open` is given. Every flag can be set by
environment instead - [DEPLOYING.md](DEPLOYING.md) lists them.

With a token, every request carries it, as `Authorization: Bearer <token>` or as the
`overseer_token` cookie. A browser gets the cookie by visiting `/auth?token=<token>&next=/` once.
Without it the answer is `401 {"error":"a token is required"}`.

### Accounts

Given an accounts file (`--accounts PATH` or `OVERSEER_ACCOUNTS`), people sign in as themselves.
The token stays what it was: whoever holds it may do anything.

- **Joining** is by invite. `/join?invite=<token>` is a form that asks for a username, a display
  name and a password, then signs the person in. An invite is used once and lasts 7 days.
  `--admin NAME` (`OVERSEER_ADMIN`) names the server's administrator: until that account exists,
  the server prints a join link for it each time it starts, and only the last one printed works.
- **Signing in**: `/signin` asks for the username and password and sets the `overseer_session`
  cookie, good for 90 days. `/signout` ends it. A browser asking without one for a page that is
  not shared with everyone is sent to `/signin`, and back afterwards.
- **`GET /account/me`** answers `{"username","name","admin","colour","icon","shape","bot"}` for whoever is signed in.
  **`POST /account/me`** with `{"name": ...}`, `{"colour": "#rrggbb"}` (or `#rgb`; `""` for the
  default), `{"icon": "bot"}` (a name from the page's icon library, `/icons/index.json`; `""` for
  none), `{"shape": "tag"}` (one of the chip shapes the specification's "Tags" lists; `""` for the
  default), `{"bot": true}` (the account is a bot; `false` for a person) or
  `{"password": ..., "current": ...}`, any of them together, changes their own display
  name, chip colour, chip icon, chip shape, bot flag or password, and answers as `GET` does - `colour`,
  `icon` and `shape` are `null` for none. A display
  name is inline markdown (see the specification's "Tags"): it shows at most 64 characters, and is
  at most 1000 as written, its markup included.
- **`POST /account/invite`**, for an admin or the token, answers `{"link": "/join?invite=...",
  "expires": ...}`. With `{"reset": "<username>"}` the link sets a new password for that account
  instead, and signs it out everywhere.
- **`POST /account/sharing`** makes the person's `sharing.os` (see below) when they have none,
  with a form that adds an entry, and answers `{"document": "<username>/sharing.os"}`; one that
  is there already is left as it is. The page's "Sharing" opens it.
- **`GET /account/sharing`** answers what each entry of the person's own `sharing.os` comes to
  now: `{"shares": [{"document", "with", "can", "documents", "people"}]}`, each entry as written
  with the documents it covers, by full name, and the accounts it is with, its groups' members
  spelt out (and `everyone`, where it says so). The sharing document's "What it comes to" shows it.
- **Acting for someone**: a request with the token may add `X-Overseer-As: <username>` to be
  that person - the bot does, for whoever is talking to it. From anyone else the header is
  ignored, and naming no account is `403`.

### Keys

A program that is to act as one person - a bot on someone's desktop, an agent that leaves
comments under its own name - is given a key of that person's account. It sends
`Authorization: Bearer <key>` and is that person, signed in: it reaches what they may and nothing
else, and `sharing.os` restricts it as it restricts them. A program that should see only some of
someone's documents gets an account of its own, by invite, and those documents are shared with it.

- **`POST /account/keys`** with `{"name": ...}`, signed in, makes one and answers
  `{"name", "key"}`. The key begins `osk_` and is shown this once; only its hash is kept. Each of
  a person's keys has a name of its own.
- **`GET /account/keys`** answers `{"keys": [{"name", "made", "used"}]}`, `used` being when it
  was last used, to within an hour, or `null`.
- **`DELETE /account/keys/<name>`** revokes one. A key lasts until it is revoked; a new password
  leaves keys as they are.

The page's "Keys", in the account drawer, does all three. A key that is not one - revoked, or
mistyped - is answered `401 {"error":"the key is not known"}`, never let in as a stranger. The
token is no account and makes no keys; only the token may send `X-Overseer-As`.

The server reached at its public address takes keys as it does on this machine, so a program
anywhere can use the deployed one this way, as long as the server is bound beyond loopback.

### Whose documents

With accounts, the root holds a folder per person, named by their username, and what is in it is
theirs. A name whose first part is a username is in that person's folder: `alice/weight.os`. Any
other name a person gives is in their own, so `tasks.os` is each person's own tasks and
`projects/plan.os` is `<me>/projects/plan.os`. The token alone works in the whole root, as without
accounts; with `X-Overseer-As` it names documents as that person does.

The page always shows the full name: a person who opens `/doc/tasks.os` finds the address bar at
`/doc/<me>/tasks.os`, the node after `#` kept, so the link they copy is the one whoever they share
it with opens. The short form is still taken, by the page and every route, and both are one
document: what is kept for a viewer - what they look at, what they hold in view - is kept under the
full name, whichever was asked.

What a person lets others reach is their `sharing.os`, an ordinary document beside their others:
a list whose entries each say a `document` in their folder, who it is `with` - a username, or
`everyone` for anyone at all, signed in or not - and what they `can` do, `read` or `edit`.
`sharing.os` itself is never shared, whatever it says.

One entry can cover more than one of each. `document` may name several documents, separated by
commas, or a folder, ending in `/`: `projects/` covers every document under it, in folders within
it too, now and later. A name with a `.` or `..` step, or `/` alone, covers nothing. `with` may
name several, separated by commas, and any of them may be a group: an entry of another list of
the same document with a `name` and its `members`, usernames separated by commas. Adding someone
to a group shares with them everything shared with it. A group's name stands for its members
even where an account has the same name, so an account made later under that name gets nothing by
it; a member is an account, never another group, nor `everyone`.

```overseer
div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
    div Group {
        string name = ""
        string members = ""
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "shopping.os"
        - with = "alice"
        - can = "edit"
    }
    - {
        - document = "recipes/"
        - with = "everyone"
    }
    - {
        - document = "projects/game.os, projects/notes.os"
        - with = "team"
        - can = "edit"
    }
    - {
        - document = "projects/game.project.os"
        - with = "team"
    }
}

list Groups (entry=<Group>, key="name") {
    - {
        - name = "team"
        - members = "bob, bob-agents"
    }
}
```

Every entry is asked, and any one that allows is enough: where entries overlap, the widest `can`
wins. So a `read` share of a document beside an `edit` share of its folder gives `edit` on the
document; documents of one folder that are to be shared differently are named in entries of their
own, as `team` above edits the trackers and only reads the definition. A folder share reaches
what a share of each of its documents would - their parts, and their folders of files.

This is asked of every file a request reaches: the document named, what it mounts and the files
its templates come from. A mount of something its reader may not read loads as a missing file
would, and the rest of the document is drawn. A write needs `edit` on the document written; a
press that only moves what the viewer is looking at needs only `read`. A write that lands inside a
mount is written to the mounted file, and needs `edit` on that file: with `edit` on
`bob/weight.os` and `read` on `ap31/foods.os`, Bob records meals of the foods there, and adding
a food through the mount is `403`. A list's parts - the files beside a document its entries are
kept in - go with the document: whoever may read it reads them, and whoever may edit it writes
them through it, though by their own names they are only ever the owner's to change.

Nobody signed in reaches only what is shared with everyone; anything else, there or not, is `401
{"error":"sign in"}`. Someone signed in is told `404 {"error":"no document '<name>'"}` of anything
missing or not shared with them, so what is there cannot be told from what is not. A write to a
document shared only to be read is `403`.

The page asks the same with its `access` command, which answers who is asking (`person`,
`stranger`, or `anyone` for the token), the person's account, and whether the open document is
theirs to `edit` or only to `read`, with its full name as `document` when they may open it. A
document shared to be read is drawn with nothing to change, whatever its own `mutable` says, and a
stranger is offered none of what they would be refused.

Two files at the root are the server's. `people.os` is the directory of everyone with an account,
`list People` keyed by `tag` (the username) with each one's `name`: the server writes it whenever
someone joins or changes their name, anyone may read it, and no request writes it, the token's
included. Documents mount it to show people by name (see "Who is looking" in the specification).
`settings.os` at the root is the look the token and anyone not signed in see; each person's own is
`settings.os` in their folder, which the drawer's choices are written to. Beside it is their
`drawer.os`, the drawer they see, made the first time the page asks for it; anyone without a folder
is given the shipped one, `library:drawer/drawer.os`, to read.

### Examples

Given a folder of examples (`--examples DIR` or `OVERSEER_EXAMPLES`; unset, there are none),
the server shows them to anyone at all, with no token or sign-in, under `/examples/`:
`/examples/games/minesweeper.os` is the page for a browser and the document for a program, as
`/doc/` is. They are a root of their own, so no name asked for there reaches the documents, and a
person's own folder called `examples` is still at `/doc/examples/`.

An example is read-only to everyone, the token's holder included. A press that only moves what the
document keeps for whoever is looking (`mutable="guarded"`) goes through, kept for that viewer
alone; one that would change the file is `403`. `access` answers `stranger` and `read`, so the
page draws it with nothing to change. Only what the page reads and presses with is served there:
`/examples/api/<command>`, `GET /examples/v1/node`, `/examples/v1/source` and
`/examples/v1/watch`. The page's own files are served to anyone too, so it can load; the list of
documents, `/doc/` and every other route stay behind the token.

A document is a `.os` file, or one under an extension that `extensions.txt`, beside the settings,
names a line each (`brot`, for Brocessor's patches): it is listed, opened and written back under
its own name like any other. Another name is refused with 400.

The examples below assume:

```bash
B=http://127.0.0.1:4747
A="Authorization: Bearer $OVERSEER_TOKEN"
J="Content-Type: application/json"
D=shopping/shopping.os
```

## Documents and addresses

A document is named by its path under the root, with forward slashes:

```bash
curl -H "$A" $B/documents
# {"documents":["basic/actions_feature_test.os", ..., "shopping/shopping.os", "tasks/tasks.os"], ...}
```

For a person, `documents` is everything they can reach, by full name, and the same is grouped:
`mine`, `shared` (by owner: `{"alice": ["alice/shopping.os"]}`) and `public`, what is shared with
everyone. Nobody signed in has no list.

`GET /doc/<name>` gives the whole resolved document as `{"nodes": [...]}`, as the page draws it:
a calendar day that only a `layout="year"` draws, marked `_drawn_by_style`, comes without its
children, which a read of it by address gives. The routes below work on one part of it.

A link to the page can name a node of the document after `#`, by its address as below:
`/doc/ap31/projects/overseer.os#project/Items/[scan]`. The page opens the document and shows that
node the way pressing a chip that stands for it does: it opens the folds and tabs around it,
scrolls to it and marks it for a moment. When the node can't be shown, the status line says why:
it isn't in the document, the filter hides it, or the page doesn't draw it. The part after `#`
never reaches the server.

A node is named by its **address**: the names from the document's top node down, joined by `/`.
It is the logical path of [the specification](overseer_syntax_specification.md#5-path-references):
a `div` with no name is no step of it, and moving fields between rows changes no address. Two
siblings of one name are `name` and `name#1`. Entries of a list that declares `key=` are named by
their key in brackets, `shopping/List/[2026-08-11T09:01:00Z]/amount`, and keep that address
whatever is added or removed around them. Entries of a list without a key are named by position -
`shopping/History/Bought__1`, `Bought__2` - and shift when one before them is removed; read the
list first and take the addresses from its answer rather than building them. A `/`, `[` or `]`
inside a name or key is escaped with `\`.

A list that keeps its entries in parts ([the specification](overseer_syntax_specification.md#keeping-a-list-in-parts))
is addressed and written as any other: the server reads its parts with the document and writes an
entry to its part, so `project/History/[stages]` means the same wherever the entry is kept. Such a
list with a `window` holds only its newest parts in the document as a whole; a read of the list, or
of anything in it, and a write to an entry, are answered with every part read.

A calendar's day is named by its date, `tracker/Days/2026-10-20/intake`, whether or not it is in
view or was ever written: a day nobody wrote reads as the calendar's template has it, and a write
to it is kept in the document, among the days in date order.

Both the document and the address go in the query string, not the path - both contain slashes.
`curl --url-query` (curl 7.87 and later) or `-G --data-urlencode` encodes them.

## Values

Values travel with their type, as the engine stores them: `{"String": "milk"}`,
`{"Integer": 2}`, `{"Float": 2.5}`, `{"Boolean": true}`, `{"Date": "2026-10-18"}`,
`{"Timestamp": "2026-10-18T12:00:00Z"}`. JSON has no infinity or NaN, so a float that is one
travels as the word a document writes it with: `{"Float": "inf"}`, `"-inf"`, `"nan"`. A bare `2` is
refused with a 400. Nodes in an answer carry their value the same way, in `parameters.value`, beside the
parameters the document gave them.

## Reading: `GET /v1/node`

```bash
curl -H "$A" -G $B/v1/node --data-urlencode document=$D \
     --data-urlencode 'address=shopping/List/[2026-08-11T09:01:00Z]/amount'
# {"address":"shopping/List/[2026-08-11T09:01:00Z]/amount","child_addresses":[],
#  "node":{"name":"amount","node_type":"float","parameters":{"value":{"Float":1.0}, ...}, ...}}
```

Every answer below has this shape: the `address` it describes, the `node` there with its
subtree, and `child_addresses`, the address of each of its children.

## Reading the text: `GET /v1/source`

```bash
curl -H "$A" -G $B/v1/source --data-urlencode document=$D
# {"document":"shopping/shopping.os","version":"3f1c9a0d27e4b8a1-10446","text":"tab shopping (...) {\r\n ..."}
```

The document's text as it is written - its own layout, comments and line endings, with what it
leaves to its templates left out - for a program that reads documents with a parser of its own.
`version` is the one `watch` reports. It reaches what any read reaches; a document's mounts and
bases are not read with it. To change a few values, use the writes below, which keep the text's
layout; to make a document, or write one whole, `PUT` it.

`parts` names the files a document's lists keep their entries in, `{}` when it has none: for each
such list, its parts in the order of their keys, under the names `/v1/source` reads them by -
`{"History": ["ap31/projects/agentloop.parts/History/2026-W44.os", ...]}`. Their entries are in
none of the document's text. Each is read as a document of its own, with its own version, and
whoever may read the document may read its parts.

`bases` names the documents its text leaves to, `[]` when it has none: the files its templates
come from, and those these come from in turn, in the order they are first named. One under the
root is named as any document is. One found in a library - the templates that ship with Overseer,
or a folder `libraries.txt` names - is named `library:` and then its path in that library, and
`/v1/source` reads it by that name, as text, with its version and `ETag`:

```bash
curl -H "$A" -G $B/v1/source --data-urlencode document=library:projects/project_template.os
# {"document":"library:projects/project_template.os","version":"...","text":"tab project (...) {\r\n ...","parts":{},"bases":[]}
```

Anyone signed in may read a library's document, whether or not any document of theirs is made
from it; it is never written. A name that climbs out of the library, or leads out of it, is
answered as one that is not there. The project template says which of its versions it is in
`/project/template_version`, an `int` raised whenever its stages, complexities, flags or the
fields of an item change, so that a program reading projects can tell its template from the one
it was written against.

The version comes as the answer's `ETag` too. A program that reads the same document again names
the `ETag` it holds, as `--url-query since=<tag>` or `If-None-Match: "<tag>"`, and while the text
is still that version it is answered `304 Not Modified`, with no body; once it changed, it gets
the text and its new version as above. For a document with parts the `ETag` is the version with
the parts there are folded in, so a new part gets the answer again even while the text stands as
it was; a past part, asked for by its own version, is answered `304` until it changes. Without
parts, the `ETag` is the version. A document it may not read is refused all the same.

## Writing the text: `PUT /v1/source`

```bash
curl -H "$A" -X PUT --data-binary @tracker.os --url-query document=projects/parser.os $B/v1/source
# 201 {"document":"alice/projects/parser.os","version":"...","created":true}
curl -H "$A" -X PUT --data-binary @tracker.os --url-query document=projects/parser.os \
     -H 'If-Match: "<version>"' $B/v1/source
# 200 {"document":"alice/projects/parser.os","version":"...","created":false}
```

The body is the document's whole text, UTF-8, and it is written byte for byte. A document that is
not there is made, with the folders on the way; it is `201`, and the answer's `ETag` is the new
version. Whoever may change a document of that name may make it: a person in their own folder,
or in a folder of someone else's that `sharing.os` lets them edit.

It never replaces a document unless told what it is replacing:

| sent | the document is there | it is not |
| --- | --- | --- |
| nothing | `412`, nothing written | made, `201` |
| `If-None-Match: *` | `412` | made, `201` |
| `If-Match: "<version>"` (or the `ETag`) | replaced, `200`, when it is still that version; else `412` | `412` |

So a program that means to start a document cannot write over one of the same name, and one that
read a document, changed its text and writes it back loses no write made in between: on `412` it
reads again and writes onto what is there now. A replacement is one undo step, as any write is.
Only the text is written: a list's parts are left as they are.

Text the parser would pass over (see "Text that cannot be read" in
[the specification](overseer_syntax_specification.md#text-that-cannot-be-read)) is refused with a
`400` that names each line, and nothing is written. A library's document is never written.

## Files beside a document: `/v1/file`

```bash
curl -H "$A" -X PUT --data-binary @drive.py "$B/v1/file?doc=projects/p.project.os&path=tests/drive.py"
# {"path":"tests/drive.py","sha256":"9f2c..."}
curl -H "$A" "$B/v1/file?doc=projects/p.project.os&path=tests/drive.py" -o drive.py
curl -H "$A" -X DELETE "$B/v1/file?doc=projects/p.project.os&path=tests/drive.py"
# {"removed":"tests/drive.py"}
```

A document keeps plain files of any kind beside it, in a folder of its own named after it:
`tools/<name>/` next to the document, so `projects/p.project.os` keeps `tests/drive.py` as
`projects/tools/p.project/tests/drive.py`. A project's definition lists the scripts its sessions
run this way, each with its hash, in its `Tools` list.

- `GET` answers with the file's bytes, as `application/octet-stream`, and their sha256 in hex as
  the `ETag`. Named in `If-None-Match`, a version still current is answered `304 Not Modified`
  with no body. It is allowed to whoever may read the document, and to anyone else is answered as
  the document itself would be: `401` to a stranger, `404` to a person it is not shared with.
- `PUT` keeps the request's body at that path, making the folders on the way and replacing what
  was there, and answers with the path and the sha256 of what it kept. `DELETE` removes the file.
  Both need the document shared to edit: a read share is `403`, and anyone it is not shared with
  `404`, as for any write. A library's document is only read.
- `path` is relative to the document's folder of files, plain names separated by `/`: no `..`, no
  root, no drive, no `:`, and nothing that leads outside the folder once links are followed - `400`, or
  `404` for a link that points away. A file not there is `404`.
- A file may weigh 8 MiB at most, as an image may. A larger body is `413` and nothing is kept; a larger file put
  there by other means is `400` and is not handed out.

## Setting a value: `POST /v1/node`

```bash
curl -H "$A" -H "$J" $B/v1/node --url-query document=$D \
     --url-query 'address=shopping/List/[2026-08-11T09:03:00Z]/commentary' \
     -d '{"value": {"String": "the mint one"}}'
```

Answers with the node it set. Without `value`: `400 {"error":"'value' is required"}`.

## Appending an entry: `POST /v1/append`

```bash
curl -H "$A" -H "$J" $B/v1/append --url-query document=$D --url-query address=shopping/List \
     -d '{"fields": {"added": {"Timestamp": "2026-10-18T12:00:00Z"},
                     "handle": {"String": "milk"}, "amount": {"Integer": 3}}}'
# {"address":"shopping/List","child_addresses":[..., "shopping/List/[2026-10-18T12:00:00Z]"], "node":{...}}
```

The entry is made from the list's `entry=` template, so a field left out takes the template's
value and is not written. A nested field is named with a slash: `"per_100g/calories"`. Answers with
the list. A field the template does not have, or a key already in the list, is a 400 that says
which.

`"with"` beside `"fields"` stacks more on the template for this entry, as a document writes it
after the dash (`- <FOODS/Catalog/[apple]> { }`, see "Stacking templates" in the specification):

```bash
curl -H "$A" -H "$J" $B/v1/append --url-query document=$D --url-query address=day/Meals \
     -d '{"with": "FOODS/Catalog/[apple]", "fields": {"portions": {"Float": 2}}}'
```

The entry then has what that reference gives under the fields it was sent, and only the
reference and those fields are written. The fields are checked against the two together, and a
reference that names nothing is a 400 that says so.

## Removing an entry: `POST /v1/remove`

```bash
curl -H "$A" -H "$J" $B/v1/remove --url-query document=$D \
     --url-query 'address=shopping/List/[2026-10-18T12:00:00Z]' \
     -d '{"fields": {"handle": {"String": "milk"}}}'
```

The body is optional. When given, its `fields` are what the entry must hold, and the removal is
refused if it does not - which is what keeps an address by position, gone stale after another
removal, from taking out the wrong entry:

```
400 {"error":"'shopping/List/[2026-10-18T12:00:00Z]' is not the entry you meant: its 'handle' is milk, not bread. ..."}
```

Answers with the list the entry came out of.

## Pressing something: `POST /v1/event`

Runs an `on <event>` block the document already declares - nothing a client sends can do more than
the document says. Here, a tap on a shopping entry, which moves it to History:

```bash
curl -H "$A" -H "$J" $B/v1/event --url-query document=$D \
     --url-query 'address=shopping/List/[2026-08-11T09:00:00Z]' -d '{"event": "click"}'
# {"address":"shopping/List","child_addresses":[...],"gone":true,"node":{...}}
```

The answer describes what the pressed node sits in, afterwards; `gone` says whether the press
removed the node itself (`node` is null when nothing is left to describe). A node with no handler
for the event is a 400 saying so. A press the document turns down with `refuse` is a 422 carrying
the document's own sentence, meant for whoever pressed, in `refused` as well as `error`:

```
422 {"error":"say an id","refused":"say an id"}
```

A `call` in the block (see the specification's "Asking the host for an operation") is the page's
to run, so a press made here runs none and the answer does not name them. The page's own press,
`run_overseer_event` through the bridge, answers with them in `calls`: each `name`, `args` worked
out, and `into` - the guarded field for what the operation answers - by `address` and by the
child indices `path` the page finds it by. An argument naming a secret textbox is no value but
`{"secret": {"address", "path"}}`: the page puts in what is typed there, which it never sends.

## Undoing: `POST /v1/undo`

```bash
curl -H "$A" -X POST $B/v1/undo --url-query document=$D
# {"steps_left":2,"undone":"shopping/shopping.os"}
```

Puts the document back as it was before its last write, whoever made it. With nothing left to
take back: `400 {"error":"there is nothing to take back for 'shopping/shopping.os'"}`.

## Watching: `GET /v1/watch`

```bash
curl -N -H "$A" $B/v1/watch --url-query document=$D
# event: changed
# data: 3f1c9a0d27e4b8a1-10446
```

A stream of server-sent events, one `changed` for each new version of the document's file: first
the version it has now, then one each time something writes it - a write through this server at
once, one made from outside (a sync, a hand edit) within a couple of seconds. Only the version: a
page that hears one asks to be caught up, and is answered against what it shows, as its own edits
are. Reading the document is enough to watch it.

A document that mounts other files, or keeps a list in parts, also gets a `read` event just before
each `changed` one. Its data names those files as they stand now, so a write to one of them is heard even though the
document's own file did not move - the version in the `changed` that follows is then the same as
before. The page's answers name the same thing as `read_from`, and a page sends it back as
`shown_read_from` when it asks to catch up, so that it is described against what it shows.

## Writes

Every write goes through the same writer as the page's edits: the document is re-written in place,
keeping its layout, its comments and the fields it left to the template, and one undo step is
recorded per write - an event's actions together are one step, the files it wrote through a mount
or to a list's parts with it. Each write is also logged as a line of `overseer-writes.jsonl` in the
documents folder.

A document may hold at most 95 MiB, just under the size at which GitHub turns a file away. A write
that would take a document, or a file it mounts, past that is a 422 that says so, and nothing is
written. A write that leaves a file already past it no larger goes through, and so does an undo.

## Errors

An error is a JSON body `{"error": "<sentence>"}` that names what was wrong and never a path on
disk:

| status | meaning |
| --- | --- |
| 400 | the request cannot be done as asked: a missing or ill-typed value, an unknown field, a duplicate key, a removal that does not match, a document name that leaves the root |
| 401 | no token, or the wrong one, and no session - with accounts, `sign in`, for anything not shared with everyone |
| 403 | not allowed to whoever is asking: an invite by someone who is no admin, `X-Overseer-As` naming no account, a write to a document shared to be read or to an example, which says why in `refused` too |
| 404 | no such document, or one not shared with whoever is asking, or nothing at that address |
| 412 | a `PUT /v1/source` found the document otherwise than it said: there when it should not be, or at another version |
| 413 | a file kept beside a document over its cap |
| 422 | the document refused the press, or the write would make it too large (`refused` holds the sentence) |
| 500 | the write failed |

## The page's bridge

`POST /api/<command>` is how the page talks to the server, with the desktop app's commands; it is
not a stable interface. The one a client uses is `save_overseer_image`, which keeps a picture
beside a document and answers with the path to write into it:

```bash
curl -H "$A" -H "$J" $B/api/save_overseer_image \
     -d '{"document": "'$D'", "args": {"path": "'$D'", "mime": "image/png", "data": "<base64>"}}'
# {"result":"images/shopping/20261018-120126.png"}
```

The server picks the file's name, under `images/<document>/`; nothing sent can place it anywhere
else. `save_overseer_audio` keeps a sound the same way, under `sounds/<document>/` - `mime` one of
`audio/mpeg`, `audio/mp4`, `audio/aac`, `audio/ogg`, `audio/opus`, `audio/webm`, `audio/wav` or
`audio/flac`, parameters such as `;codecs=opus` allowed, and at most 16 MB.

`show_more_of_overseer_list` (`list_path`, the list's path of names) is the page asking a
windowed list for one window more. It is kept against the session that asked, writes nothing,
and answers with the whole document as that session now sees it. With `until`, a key of the list,
it asks for as many windows more as it takes to draw the entry with that key - or until the list
leaves nothing out - and answers once.
