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
but loopback, it refuses to start without one.

With a token, every request carries it, as `Authorization: Bearer <token>` or as the
`overseer_token` cookie. A browser gets the cookie by visiting `/auth?token=<token>&next=/` once.
Without it the answer is `401 {"error":"a token is required"}`.

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
# {"documents":["basic/actions_feature_test.os", ..., "shopping/shopping.os", "tasks/tasks.os"]}
```

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
`{"Timestamp": "2026-10-18T12:00:00Z"}`. A bare `2` is refused with a 400. Nodes in an answer carry their value the same way, in `parameters.value`, beside the
parameters the document gave them.

## Reading: `GET /v1/node`

```bash
curl -H "$A" -G $B/v1/node --data-urlencode document=$D \
     --data-urlencode 'address=shopping/List/[2026-08-11T09:01:00Z]/amount'
# {"address":"shopping/List/[2026-08-11T09:01:00Z]/amount","child_addresses":[],
#  "node":{"name":"amount","node_type":"float","parameters":{"value":{"Integer":1}, ...}, ...}}
```

Every answer below has this shape: the `address` it describes, the `node` there with its
subtree, and `child_addresses`, the address of each of its children.

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
| 401 | no token, or the wrong one |
| 404 | no such document, or nothing at that address |
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
