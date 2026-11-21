---
name: overseer-new-tracker
description: Start an Overseer project tracker for a local repository on a remote Overseer server - read the repository, write its definition and its tracker, carry over the open entries of a local tracker such as docs/todo.md, and push both documents with the person's key. Use when asked to start, initiate, create or set up an Overseer tracker (or "project on Overseer", "tracker for AgentLoop") for a repository.
---

# Starting an Overseer tracker for a repository

A tracker is two documents side by side on the server, in the person's folder:

| document | made from | holds |
| --- | --- | --- |
| `projects/<name>.project.os` | `projects/project_definition.os/definition` | what the project is: description, repositories, places, tools, systems |
| `projects/<name>.os` | `projects/project_template.os/project` | the work: items, discussion, history; it mounts the definition |

AgentLoop reads exactly this pair: the tracker's address, and the definition through it at
`project/definition/definition`. Both templates ship with every Overseer server, so these two
files write only what is the project's own. For the language beyond what is here, the
`overseer-documents` skill, if it is installed.

## 1. What to ask

- **The server's address**, e.g. `https://overseer.example.org` or `http://tower:4747`. Ask; never
  guess. Different repositories may use different servers.
- **The key file.** Default `~/.overseer/key`; ask whether to use another. It holds one line, a key
  of the person's own account (`osk_...`, made in the page's account drawer under "Keys"). If it
  is not there, tell the person to make the key and write the file themselves
  (`chmod 600` it), and wait. **Never ask for the key in the chat, never print it, never write it
  anywhere**, and never use the server's master token in its place.
- **The name**, default the repository folder's name in lowercase. It names both files.

Every request below sends the key from the file, so it never appears in a command:

```bash
B=https://overseer.example.org          # the server, no trailing slash
A="Authorization: Bearer $(cat ~/.overseer/key)"
```

## 2. Check before writing anything

```bash
curl -sS -H "$A" $B/account/me                          # 200: whose folder this lands in
curl -sS -o /dev/null -w '%{http_code}\n' -H "$A" -G $B/v1/source \
     --data-urlencode document=projects/<name>.os        # 404 is what we want
```

- `401` on the first: the key is wrong or revoked. Stop and say so.
- `200` on the second (or for `<name>.project.os`): a tracker of that name is there. Stop, say
  so, and ask for another name. This skill only starts trackers; it never changes one.

## 3. Read the repository

Read what it says about itself before deciding anything: `README*`, `CLAUDE.md`, `AGENTS.md`,
`docs/about.md`, `docs/rules.md`, the build files (`package.json`, `Cargo.toml`, `pyproject.toml`,
...), CI workflows, and `git remote -v` with `git rev-parse --abbrev-ref HEAD`. Take the remote's
URL without any credentials in it; an SSH remote stays as it is.

## 4. The definition: `projects/<name>.project.os`

```overseer
<projects/project_definition.os/definition> definition {
    - description = "<what the project is and holds to>\n\n**Where it is.** ...\n\n**Checking work.** ...\n\n**What has bitten before.** ..."
    - Repos {
        - {
            - name = "<name>"
            - about = "<one line>"
            - url = "<clone URL, no credentials>"
        }
    }
    - systems = "linux, macos"
}
```

- `description` is markdown, four short paragraphs as above, from what the repository says - its
  intent, where its parts are, the exact build and test commands, its known traps. `{<repo>}` and
  `{<place>}` stand for where a repository or place is checked out, as AgentLoop fills them in:
  "`cargo test` in `{overseer}`".
- `Repos`: one entry per repository the work touches, usually just this one. `branch` only when it
  is not the default.
- `Places` (entries of `name` and `about`): folders outside the repositories the work reads or
  writes. Leave the list out when there are none.
- `systems`: what it is built and run on, out of `windows`, `macos`, `linux`, comma-separated.
- Leave `Tools` and `settings` out. Tools are files kept on the server beside the definition, which
  the person adds through AgentLoop.

## 5. The tracker: `projects/<name>.os`

```overseer
<projects/project_template.os/project> project {
    - definition (source="<name>.project.os/definition", lazy=false)
    - name = "<Name as people say it>"
    - description = "<one sentence>"
    - Items {
        - {
            - added = "2026-11-21T14:05:00+01:00"
            - handle = "T-04"
            - title = "<one line of intent>"
        }
    }
}
```

Leave `Items` out when there is nothing to carry over. Every other list - stages, labels,
milestones, history - starts as the template has it.

### Carrying over a local tracker

Look for one: `docs/todo.md`, else `TODO.md` or `todo.md` at the root. If there is one, every open
entry becomes an item, in the order it is written:

| in the file | in the item |
| --- | --- |
| the id (`T-04`) | `handle`, as written. An entry without an id gets a short lowercase handle of its own |
| the line of intent | `title`, without the checkbox and id; markdown kept |
| indented sub-bullets, `Done:` and `Left:` lines | one comment holding them, as markdown (below) |
| "blocked on T-03", "waits on T-03" | `after = "T-03"` when T-03 is an item too; else it stays in the comment |
| `[x]` ticked | left out: it is done |

```overseer
            div Talk {
                list Comments {
                    - {
                        - at = "<the same timestamp as added>"
                        - author = "owner"
                        - body = "- first sub-bullet\n- Left: what remains"
                    }
                }
            }
```

`added` is now, with the local offset (`date +%Y-%m-%dT%H:%M:%S%z`, with a colon put into the
offset), the same for every item. Leave `stage` out (it starts as `filed`), and `labels`,
`milestones` and `points` too: the template's vocabularies are empty until the person makes them.
The archive (`docs/archive/`) is history and is not carried over.

Strings are double-quoted; inside them `\"`, `\\` and `\n` are the escapes. Indent four spaces.

## 6. Push

The definition first, since the tracker mounts it. `If-None-Match: *` makes the server refuse
rather than overwrite, should either name have been taken since step 2:

```bash
curl -sS -X PUT -H "$A" -H 'If-None-Match: *' --data-binary @<name>.project.os \
     --url-query document=projects/<name>.project.os $B/v1/source
curl -sS -X PUT -H "$A" -H 'If-None-Match: *' --data-binary @<name>.os \
     --url-query document=projects/<name>.os $B/v1/source
```

Write the two files into a scratch folder first, not into the repository. `201` is made. `412` is
taken: stop and say which. `400` names the lines the server could not read: fix them and push
that document again. A `404` for the route itself is a server older than `PUT /v1/source`: say it
needs updating.

## 7. Check, then hand over

```bash
curl -sS -H "$A" -G $B/v1/node --data-urlencode document=projects/<name>.os \
     --data-urlencode address=project/definition/definition     # the definition, through the mount
curl -sS -H "$A" -G $B/v1/node --data-urlencode document=projects/<name>.os \
     --data-urlencode address=project/Items                     # child_addresses: one per item
```

Both must answer `200`, and `Items` must have as many entries as were carried over. Then tell the
person:

- the tracker's page, `<B>/doc/<their username>/projects/<name>.os`, which is also the address an
  AgentLoop loop takes;
- what was carried over, and from which file. The local file is left as it was: whether to retire
  it is theirs to decide;
- that a loop's account reaches the tracker only once it is shared with it, an entry in their
  `sharing.os` with `edit`.
