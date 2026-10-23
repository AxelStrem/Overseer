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
- **`GET /account/me`** answers `{"username","name","admin"}` for whoever is signed in.
  **`POST /account/me`** with `{"name": ...}`, or `{"password": ..., "current": ...}`, changes
  their own display name or password.
- **`POST /account/invite`**, for an admin or the token, answers `{"link": "/join?invite=...",
  "expires": ...}`. With `{"reset": "<username>"}` the link sets a new password for that account
  instead, and signs it out everywhere.
- **`POST /account/sharing`** makes the person's `sharing.os` (see below) when they have none,
  with a form that adds an entry, and answers `{"document": "<username>/sharing.os"}`; one that
  is there already is left as it is. The page's "Sharing" opens it.
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

What a person lets others reach is their `sharing.os`, an ordinary document beside their others:
a list whose entries each say a `document` in their folder, who it is `with` - a username, or
`everyone` for anyone at all, signed in or not - and what they `can` do, `read` or `edit`.
`sharing.os` itself is never shared, whatever it says.

```overseer
div (hidden=true) {
    div Share {
        string document = ""
        string with = ""
        string can = "read"
    }
}

list Shares (entry=<Share>) {
    - {
        - document = "shopping.os"
        - with = "alice"
        - can = "edit"
    }
    - {
        - document = "recipes.os"
        - with = "everyone"
    }
}
```

This is asked of every file a request reaches: the document named, what it mounts and the files
its templates come from. A mount of something its reader may not read loads as a missing file
would, and the rest of the document is drawn. A write needs `edit` on the document written; a
press that only moves what the viewer is looking at needs only `read`. A write that lands inside a
mount is written to the mounted file, and needs `edit` on that file: with `edit` on
`bob/weight.os` and `read` on `ap31/foods.os`, Bob records meals of the foods there, and adding
a food through the mount is `403`.

Nobody signed in reaches only what is shared with everyone; anything else, there or not, is `401
{"error":"sign in"}`. Someone signed in is told `404 {"error":"no document '<name>'"}` of anything
missing or not shared with them, so what is there cannot be told from what is not. A write to a
document shared only to be read is `403`.

The page asks the same with its `access` command, which answers who is asking (`person`,
`stranger`, or `anyone` for the token), the person's account, and whether the open document is
theirs to `edit` or only to `read`. A document shared to be read is drawn with nothing to change,
whatever its own `mutable` says, and a stranger is offered none of what they would be refused.

Two files at the root are the server's. `people.os` is the directory of everyone with an account,
`list People` keyed by `tag` (the username) with each one's `name`: the server writes it whenever
someone joins or changes their name, anyone may read it, and no request writes it, the token's
included. Documents mount it to show people by name (see "Who is looking" in the specification).
`settings.os` at the root is the look the token and anyone not signed in see; each person's own is
`settings.os` in their folder, which the drawer's choices are written to.

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

`GET /doc/<name>` gives the whole resolved document as `{"nodes": [...]}`; the routes below work
on one part of it.

A node is named by its **address**: the names from the document's top node down, joined by `/`.
It is the logical path of [the specification](overseer_syntax_specification.md#5-path-references):
a `div` with no name is no step of it, and moving fields between rows changes no address. Two
siblings of one name are `name` and `name#1`. Entries of a list that declares `key=` are named by
their key in brackets, `shopping/List/[2026-08-11T09:01:00Z]/amount`, and keep that address
whatever is added or removed around them. Entries of a list without a key are named by position -
`shopping/History/Bought__1`, `Bought__2` - and shift when one before them is removed; read the
list first and take the addresses from its answer rather than building them. A `/`, `[` or `]`
inside a name or key is escaped with `\`.

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
bases are not read with it. It is only for reading: changes go through the writes below, which
keep the text's layout.

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

## Writes

Every write goes through the same writer as the page's edits: the document is re-written in place,
keeping its layout, its comments and the fields it left to the template, and one undo step is
recorded per write - an event's actions together are one step. Each write is also logged as a line
of `overseer-writes.jsonl` in the documents folder.

## Errors

An error is a JSON body `{"error": "<sentence>"}` that names what was wrong and never a path on
disk:

| status | meaning |
| --- | --- |
| 400 | the request cannot be done as asked: a missing or ill-typed value, an unknown field, a duplicate key, a removal that does not match, a document name that leaves the root |
| 401 | no token, or the wrong one, and no session - with accounts, `sign in`, for anything not shared with everyone |
| 403 | not allowed to whoever is asking: an invite by someone who is no admin, `X-Overseer-As` naming no account, a write to a document shared to be read |
| 404 | no such document, or one not shared with whoever is asking, or nothing at that address |
| 422 | the document refused the press (`refused` holds its sentence) |
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
else.

`show_more_of_overseer_list` (`list_path`, the list's path of names) is the page asking a
windowed list for one window more. It is kept against the session that asked, writes nothing,
and answers with the whole document as that session now sees it.
