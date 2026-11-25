---
name: overseer-new-server
description: Stand up a new Overseer server that is ready for people and AgentLoop - choose its settings, start it, make the admin and agents' accounts, keys and sharing, and check what the server keeps by itself (people.os) - or check a server that is already running against the list of what goes missing. Use when asked to deploy, raise, set up, start or audit an Overseer server, or when trackers, people's names, keys or sharing misbehave on one.
---

# Standing up an Overseer server

A server is `overseer-server` serving one folder of documents, with the page, the API and the
templates that ship with Overseer (`library/`). Deploying it is quick. What goes wrong is what a
fresh server lacks: accounts, the agents' account, sharing, a cache that survives restarts. Also
what people do to fill the gap, such as copying a template or writing `people.os` by hand.
[DEPLOYING.md](../../../DEPLOYING.md) has every setting; [API.md](../../../API.md) has the routes.
This skill is the order to do things in, and what to check.

**The rule for everything below: what the server writes, nobody writes.**
- `people.os` is written by the server.
- Each person's `drawer.os` is made by the page.
- `sharing.os` is made by "Sharing" in the account drawer, or `POST /account/sharing`.
- The templates come from the library.

None of these is made by hand or copied in.

## 1. What to ask

- **Where it runs.** On a machine at home (the binary or the image), on Unraid, or on a hosting
  service such as Railway, Render or Fly that builds the `Dockerfile`.
- **Its address**, as people will reach it: a domain with HTTPS, or `http://<host>:4747` on a
  network.
- **The admin's username**: the person's own, lowercase.
- **The time zone** dates are read in, e.g. `Europe/Berlin`.
- **Whether agents will work on it.** If they will, it needs an account for them (step 5).

Never ask for a password, a token or a key in the chat; never print one or write one into a file
of the repository. The person sets secrets in the host's settings or in an `.env` themselves.

## 2. The settings

Run with accounts. Without them nobody signs in, there are no keys and no `people.os`, and every
program would need the server's token.

| variable | value | why |
| --- | --- | --- |
| `OVERSEER_ACCOUNTS` | a file **outside** the documents folder | people sign in; it holds password hashes and need not travel with the documents |
| `OVERSEER_ADMIN` | the admin's username | until that account exists, each start prints a link to make it |
| `OVERSEER_TOKEN` | `openssl rand -hex 32`, kept by the person | for emergencies only, never for a program |
| `TZ` | the time zone | dates in documents are read in it; otherwise UTC |
| `OVERSEER_DISK_CACHE` | a folder on the volume, outside the documents | a restart, deploy or wake-up reads back what it had instead of working everything out again |
| `OVERSEER_ROOT` | the documents folder | only when the default (`/data` in the image) is not where the documents go |

Where the files go:
- **A host with one volume** (Railway and the like): mount it at `/data` and split it:
  - `OVERSEER_ROOT=/data/documents`
  - `OVERSEER_ACCOUNTS=/data/overseer-accounts.json`
  - `OVERSEER_DISK_CACHE=/data/overseer-cache`

  The image's entrypoint creates these folders and hands them to the user it runs as. Without a
  volume, the documents are lost on the next deploy.
- **Unraid or compose**: three mounts, `/data`, `/config` and `/cache`, as
  `docker/unraid/overseer.xml` and `docker/compose.yaml` have them. `PUID`/`PGID` default to
  `99:100`.
- **The binary alone**: also give `--frontend` (the release's `overseer-page-<tag>.zip`, unpacked)
  and `--bind 0.0.0.0`. On Linux, set `MALLOC_ARENA_MAX=2`; the image already does.

Memory (`OVERSEER_CACHE_MB`, `OVERSEER_IDLE_MINUTES`) can stay at the defaults until the
`memory` lines in the log say otherwise. Use a release, not a branch: the image
`ghcr.io/axelstrem/overseer:<version>`, or a release's binary. AgentLoop checks the project
template version the server ships against the versions it knows (section 8, item 1).

## 3. Start it and make the admin

Start it, then find the join link in the log, e.g. `docker logs overseer 2>&1 | grep join`, or
the host's log view:

```
.../join?invite=...
```

**The person opens it and makes their account.** It's a form with a password, so it's theirs to
fill in, not yours. Only the newest link printed works, and the link stops being printed once
the account exists.

Then check, with the token held in a file and never in the command:

```bash
B=https://overseer.example.org
T="Authorization: Bearer $(cat ~/.overseer/token)"     # the server's token, emergencies only
curl -sS -H "$T" -G $B/v1/source --data-urlencode document=people.os
```

The answer's `text` must list the admin under `list People`. Once accounts are on, the server
rewrites `people.os` itself at every start, whenever someone joins, and whenever anyone changes
their name, colour, icon, shape or bot flag. **No request can write it**, not even with the token, and it must never be copied
in from another server: the next change to accounts overwrites it anyway.

From here on, use a key, not the token. The person makes one in the page's account drawer under
"Keys" and saves it as `~/.overseer/key` (`chmod 600`).

## 4. What each person has

Each account has a folder named by its username. Everything there is theirs. All of it is made
as it is used, so none of it is written in advance:

| file | made by |
| --- | --- |
| `<user>/drawer.os` | the page, the first time it draws their drawer; it inherits `library:drawer/drawer.os` |
| `<user>/settings.os` | the drawer's choices: theme, look |
| `<user>/sharing.os` | "Sharing" in the account drawer, or `POST /account/sharing`; one already there is left alone |

To share anything, someone needs a `sharing.os`. Make it through the route rather than writing
it, so it has the form the page expects:

```bash
A="Authorization: Bearer $(cat ~/.overseer/key)"
curl -sS -X POST -H "$A" $B/account/sharing          # {"document": "<user>/sharing.os"}
```

## 5. The agents' account

Agents comment and move items under an account of their own. This keeps their edits apart from
the person's, and they see only what is shared with them.

1. **Invite it**, as the admin: the page's account drawer, or
   `curl -sS -X POST -H "$A" $B/account/invite`, which answers a `link`. The person opens the
   link in a private window and makes the account, e.g. `<user>-agents`.
2. **Mark it as a bot.** Signed in as that account, tick "This account is a bot" in the account
   drawer, or send
   `POST /account/me` with `{"bot": true}` using its key. `people.os` then says `bot = true`, and
   pages show it as a bot.
3. **Make its key** while signed in as it, under "Keys", saved as e.g. `~/.overseer/agents.key`.
   This is the key AgentLoop's worker takes.
4. **Share with it**, in the person's `sharing.os`, for each project:
   - the tracker, `projects/<name>.os`, with `edit`;
   - its definition, `projects/<name>.project.os`, with `read`;
   - any tracker it files into.

   A group is easier once there are several agent accounts:
   `- { - name = "agents"  - members = "<user>-agents" }`, with the shares made `with = "agents"`.

Check the agents can see their trackers, using the agents' key:

```bash
curl -sS -H "Authorization: Bearer $(cat ~/.overseer/agents.key)" $B/documents
```

`shared` must list each tracker and its definition.

## 6. Trackers

Start each one with the `overseer-new-tracker` skill. It makes the tracker and its definition
from the shipped templates, with the `mount people` the tracker needs.

There's nothing to set up for templates. The server carries `library/` (`OVERSEER_LIBRARY`,
`/app/library` in the image), and `<projects/project_template.os/project>` finds it from any
folder. Putting a copy of a template in the documents does harm (section 8, item 6).

## 7. Hand over

Tell the person:
- the page's address, and that new people join by invites the admin makes;
- where the token is, and that it's for emergencies;
- the agents' account and its key file, and what is shared with it;
- that `people.os`, `drawer.os` and the cache look after themselves.

## 8. A server already running: what goes missing

Go through this in order on any server that's been up a while, or one where something misbehaves.
Each item says how to check it and how to fix it.

1. **The release.** The image tag or binary should be a release, recent enough for the AgentLoop
   people run.
   - Check: read the shipped template with any signed-in key:
     `GET /v1/source?document=library:projects/project_template.os`. The `text` holds
     `int template_version ... = <n>`, and that `<n>` must be in AgentLoop's `TEMPLATE_VERSIONS`
     (`src/agentloop/tools/tracker.py`).
   - Symptom: `tracker.py` stops with a line naming two versions.
   - Fix: update the older side, server or AgentLoop.
2. **Accounts are on.**
   - Check: `GET /account/me` with a key answers a username.
   - Symptom: a server run with only a token or `--open` has no `people.os`, no keys and no
     sharing.
   - Fix: set `OVERSEER_ACCOUNTS` and `OVERSEER_ADMIN`, and restart. Existing documents stay
     where they are, but with accounts each person's documents live in their own folder, so
     move them under `<admin>/`.
3. **`people.os` is the server's.**
   - Check: read it with the token (step 3). It should list every account and start with the
     line `// Written by the server from its accounts`.
   - Symptom: a hand-made or copied one shows people who aren't there, and misses those who are.
   - Fix: restart the server, or change anything about an account. The server rewrites the file
     at every start and after every account change. Never edit it.
4. **The accounts file and cache sit outside the documents, on the volume.**
   - Check: the paths in the settings.
   - Symptom: a cache in the container's temp folder is lost on every start. That's slow, not
     broken. An accounts file without a volume loses everyone's accounts on a deploy.
5. **Every tracker mounts the people.**
   - Check the text of each `projects/<name>.os` (`/v1/source`) for
     `mount people (source="../../people.os/People", ...)`. Then read `project/people/People`
     with `/v1/node`: it should list the accounts.
   - Symptom: owners and assignees show as bare usernames, and AgentLoop doesn't route items by
     mention.
   - Fix: add the line after `- description`.
6. **Templates come from the library, not a copy.**
   - Check: `GET /v1/source` of each tracker answers `bases`, and those must start with
     `library:`, e.g. `["library:projects/project_template.os"]`. A base named under the root,
     like `ap31/projects/projects/project_template.os`, is a copy that has taken over from the
     shipped template. A `libraries.txt` beside a `settings.os` can do the same through a folder
     it names. Also list `GET /documents` with the token and look for any `project_template.os`,
     `project_definition.os` or `drawer.os` copied out of the library.
   - Symptom: the tracker is frozen at the copy's version.
   - Fix: delete the copy, with the owner's agreement. The tracker then inherits from the
     library again. If the copy had its own changes, they belong in Overseer's `library/`, not
     in a copy.
7. **Each tracker's lists are written.**
   - Check: the text holds `- Items`, `- Labels` and `- History`, each as a block or `{ }`.
   - Symptom: AgentLoop's `tracker.py` stops at `no list History`.
8. **Sharing reaches the agents.**
   - Check: `GET /documents` with the agents' key lists each tracker it works and that tracker's
     definition.
   - Symptom: a missing definition still draws the tracker, but a loop finds no repositories. A
     missing tracker gives `404`.
   - Fix: entries in the owner's `sharing.os`; `GET /account/sharing` shows what each entry
     covers.
9. **Agent accounts are bots.**
   - Check: in `people.os`, each agents' account has `bot = true`.
   - Fix: step 5.2.
10. **Programs use keys, not the token.**
    - Check: `GET /account/keys` for each account lists the keys and when each was last `used`.
      A key never used, or one belonging to a machine that's gone, should be revoked
      (`DELETE /account/keys/<name>`).
    - Symptom: a bot or loop sending the token can do anything, as anyone.
11. **The admin link.**
    - Check: the log printing a join link at every start means `OVERSEER_ADMIN` names an account
      that doesn't exist. It may be misspelt, or the account was made under another name.
    - Fix: correct the variable, or make the account from the newest link.
12. **The time zone.**
    - Check: `TZ` is set.
    - Symptom: dates near midnight land on the wrong day, and calendars and "today" are off by
      the server's offset.

Say what you found item by item. Don't fix anything that writes to someone's documents without
asking, and never write `people.os`.
