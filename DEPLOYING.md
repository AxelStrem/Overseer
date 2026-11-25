# Deploying Overseer

Overseer runs three ways: as a desktop app over documents on your own disk, as a server on a
machine at home, or as a server on a hosting service. The server is `overseer-server`, built from
`src-tauri` with the `server` feature; it serves the documents in one folder, the JSON API
([API.md](API.md)) and, given a built page, the same page the desktop app draws.

## The desktop app

```bash
npm install
npm run tauri:dev        # or `npm run tauri:build` for an installer
```

It opens documents from your disk and needs nothing else. See [DEVELOPMENT.md](DEVELOPMENT.md)
for the toolchain.

## A server at home

Build the page and the server, then point the server at a folder of documents:

```bash
npm install && npm run build
cd src-tauri && cargo build --release --features server --bin overseer-server
./target/release/overseer-server --root /srv/documents --frontend ../dist --bind 0.0.0.0 --open
```

A tagged release on GitHub carries a prebuilt server for each system
(`overseer-server-<tag>-<target>`, for Linux x86-64, Windows x86-64 and macOS on both Apple
silicon and Intel) and the built page (`overseer-page-<tag>.zip`). Unpack the page into a folder
and give it as `--frontend`, and nothing has to be built.

Every machine on the network can then open `http://<server>:4747/` and read and change the
documents. `--open` is what allows that: without a token the server refuses to listen anywhere
but `127.0.0.1` unless it is asked to by name, and it prints a warning when it is. Leave `--open`
out and set `OVERSEER_TOKEN` instead to require a token on the network too.

Or with the image, from the root of this repository:

```bash
docker build -t overseer .
docker run -d -p 4747:4747 -v /srv/documents:/data -e OVERSEER_OPEN=1 overseer
```

## Unraid

Each release is an image, `ghcr.io/axelstrem/overseer:<version>` and `:latest`, for
`linux/amd64`. It runs with accounts: people sign in, and each program gets a key of an account
rather than the server's token.

- **Template**: on the Docker tab, under Template repositories, add
  `https://github.com/AxelStrem/Overseer/tree/master/docker/unraid` and save (or put
  [`docker/unraid/overseer.xml`](docker/unraid/overseer.xml) in
  `/boot/config/plugins/dockerMan/templates-user/`). Then Add Container and pick `overseer`.
- **Compose** (the Compose Manager plugin): [`docker/compose.yaml`](docker/compose.yaml), with
  `OVERSEER_TOKEN` and `OVERSEER_ADMIN` in an `.env` beside it, as its header says.

Either way:

1. Set `OVERSEER_TOKEN` to `openssl rand -hex 32` and keep it; it is for emergencies, never for
   a program. Set `OVERSEER_ADMIN` to your username and `TZ` to your time zone.
2. Start it, and find the join link in the log: `docker logs overseer 2>&1 | grep join`. Open
   `http://<tower>:4747/join?invite=...` from it, and make the account.
3. Keys for programs - Claude, AgentLoop - are made in the page's account drawer, under "Keys".
   A loop that should see only some documents gets an account of its own, by an invite from
   yours, and those documents are shared with it in your `sharing.os`.

The files belong to `PUID:PGID`, `99:100` by default, Unraid's `nobody:users`. From outside the
network, put it behind a reverse proxy with HTTPS, or reach it over a VPN such as Tailscale.

## A hosting service

Railway, Render, Fly, Heroku and the like can build the `Dockerfile` in this repository directly.

1. Attach a volume at `/data`. Without one the documents live inside the container and are lost
   on the next deploy.
2. Decide who may read them:
   - `OVERSEER_TOKEN` set to a long random string requires it on every request, as
     `Authorization: Bearer <token>` or as the `overseer_token` cookie. A browser gets the cookie
     by opening `https://<your-domain>/auth?token=<token>` once.
   - `OVERSEER_OPEN=1` serves without one, to anyone who finds the address.
   - `OVERSEER_ACCOUNTS=/data/overseer-accounts.json` lets people sign in as themselves, and
     `OVERSEER_ADMIN=<username>` names who may invite them: until that account exists, the
     server's log holds a link to make it. Keep the file out of the documents folder - it holds
     password hashes, and it need not travel with a backup of the documents. A token can be set
     beside it, for a program such as a bot; see [API.md](API.md). With accounts, each person's
     documents are in a folder of the documents named by their username, and they share them
     through a `sharing.os` there - see "Whose documents" in [API.md](API.md).

   With none of these, the server refuses to start and says which to set.
3. Nothing to do about the port: the host hands the container `PORT`, and the server listens on
   it.

To put documents on the volume, copy them in with the host's tools, or start empty and create
them from the page.

## Memory, and letting the service sleep

A document the server has worked out is kept in memory, so that opening it again is quick, up to
a budget; past it, the least recently used is let go and written to a folder on disk, from which
reading it back costs about a third of working it out again. On a host that charges for memory,
these are the settings to turn:

- `OVERSEER_CACHE_MB` - how much worked-out documents may take in memory. 64 for the server,
  512 for the desktop app; `0` keeps nothing, and every request works its document out afresh.
  Beside it the server keeps the parsed parts of lists, the mounted files worked out and the
  parsed bases, each up to a quarter of this budget and never less than 16 MB; past that, what
  was used least lately is read from its file again when next asked for.
- `OVERSEER_DISK_CACHE` - the folder for what is let go, by default `overseer-cache` in the
  system's temporary folder. Set it empty to keep nothing on disk. It holds the summaries of a
  list's parts too, in `sums/`, which spare a server that has just started reading old parts.
  A container's temporary folder starts empty every time it starts, so on a host with a volume
  put it there, outside the documents folder - `/data/overseer-cache` beside `/data/documents`.
  A restart, or a wake-up from sleep, then reads back what it had instead of working it out
  again. Its files are only ever `.json`. They are named by the build of the server, so a deploy
  of a new one finds none it can use, and the old ones go as the folder is trimmed to its budget.
  On Linux the server tells the kernel not to keep these files in its file cache once they are
  written or read, so the folder adds nothing to the container's `file` memory. That does not
  work on a folder held in memory (tmpfs): its files show as `shmem` and stay there.
- `OVERSEER_DISK_CACHE_MB` - how much that folder may hold, 512 by default; `0` turns it off. A
  large worked-out document takes 12-15 MB there, and the summaries up to an eighth of the
  budget. 256 is room for about fifteen such documents: enough for one person's, and better on
  a small volume, which the folder shares with the documents and any history of them.
- `OVERSEER_IDLE_MINUTES` - how long a document may go unused before it leaves memory, 30 by
  default; `0` keeps it for as long as there is room. Used means read, pressed, written, or
  caught up by a page that is shown: a tab left open on a document asks nothing, and does not
  keep it. Checked once a minute; what goes - the document, and the parts, mounted files and
  bases nothing has read meanwhile - is written to the disk folder as above, and the memory is
  handed back to the system. The first request after a quiet stretch reads the document back
  from there, and an open page goes on catching up by its changes.
- `MALLOC_ARENA_MAX` - how many heaps the C allocator keeps. The image sets `2`: by default it
  keeps one per busy thread and never gives them back, and a server that holds little at rest
  can sit on gigabytes. Without it, set it yourself when running the binary on Linux.

The server also hands free memory back to the system after every request.

To see where the memory goes, read the server's log. Every 15 minutes, it prints a `memory` line:
- what the server holds now and the most it has held (`rss`);
- what of that its own allocations hold (`heap`), with the most they held since the last line
  and since start. `rss` is larger by what the allocator keeps free for later and by the
  program itself; a spike on the host's graph shows as the peak since the last line in the
  first line printed after it;
- on Linux, what the container holds in all, and what that is made of:
  - `anon`, the programs' own memory;
  - `file`, the file cache, with how much of it the kernel takes back first when it needs the
    room (`inactive`) and how much it would rather keep (`active`), and the parts of those still
    to be written to the disk (`dirty`) or held in memory-only files (`shmem`);
  - `kernel`, what the kernel keeps for the container - mostly `slab`, its records of the files
    and folders read, of which `reclaimable` it can give up as easily as the file cache;
  - `sock`, network buffers, and `other`, whatever of the total the rest leave unnamed;

  a part under 4 MB is left out. Where the container's cgroup is the older kind (v1), the line
  says only `anon` and `file`;
- what the other processes in the container hold, by name;
- what each of the server's caches is keeping. Beside the documents, parts, mounted files and
  bases there are two more:
  - `frames`: the copies of mounted files that a host works its mounted nodes' formulas out in.
    A frame is held once for the whole server, and goes when the mounted file it was made from
    goes.
  - `snapshots`: the source text of the last parses (up to 32 documents), which a save uses to
    give back the bytes it was given. It is counted in the documents' figure as well, while a
    document is held.

  Each figure is an estimate that errs high, by a quarter or more.

The same line is printed after a request that took more than five seconds (once a minute at
most), and after one that grew the heap by more than 200 MB however quickly it ran, starting with
what the request did:

```
memory after /v1/event took 5.2 s, grew 840 MB (heap 300 -> 1140, alongside /doc/tasks): rss ...
```

`grew` is how far the heap rose above what it held when the request began: the most the server
held at any moment while it ran, less what it held at the start. The heap is one for all
requests, so when others ran at the same time their allocations are in the figure too, and
`alongside` names them; with nothing alongside, the figure is the request's own.

`OVERSEER_MEMORY_LOG` sets how many minutes apart the lines are, and `0` stops them all.
`OVERSEER_MEMORY_GROWTH_MB` sets how much a request has to grow the heap to be printed, and `0`
prints the line after every request.

Stopping the whole service when nobody uses it - Railway's serverless mode, Fly's auto-stop,
Render's free tier - is a setting of the host, not of Overseer, and it is safe to turn on: the
documents are on the volume, and the first request after a wake-up reads its documents back
from the disk cache if that is on the volume too, or works them out again if not.

## Every setting

A flag wins over its variable.

| flag | variable | default | |
| --- | --- | --- | --- |
| `--root DIR` | `OVERSEER_ROOT` | `../examples` (`/data` in the image) | the folder of documents; the server keeps its own settings file there too |
| `--frontend DIR` | `OVERSEER_FRONTEND` | `../dist` (`/app/frontend` in the image) | the built page; without one only the API is served |
| `--bind ADDR` | `OVERSEER_BIND` | `127.0.0.1` (`0.0.0.0` in the image) | the address to listen on |
| `--port N` | `OVERSEER_PORT`, then `PORT` | `4747` | the port |
| `--token-file PATH` | `OVERSEER_TOKEN` | none | the token a request has to carry |
| `--open` | `OVERSEER_OPEN=1` | off | serve beyond `127.0.0.1` without a token |
| `--accounts PATH` | `OVERSEER_ACCOUNTS` | none | the file of accounts people sign in with; without it, nobody signs in |
| `--admin NAME` | `OVERSEER_ADMIN` | none | the account that may invite others, made by a link printed at start-up |
| `--examples DIR` | `OVERSEER_EXAMPLES` | none | example documents shown to anyone at `/examples/`, with no sign-in, read-only; unset, none are |
| | `OVERSEER_LIBRARY` | the checkout it was built from, else `library` beside the binary (`/app/library` in the image) | the templates shipped in `library/` |
| | `OVERSEER_CACHE_MB` | `64` | see above |
| | `OVERSEER_DISK_CACHE` | system temp folder | see above |
| | `OVERSEER_DISK_CACHE_MB` | `512` | see above |
| | `OVERSEER_IDLE_MINUTES` | `30` | see above |
| | `OVERSEER_MEMORY_LOG` | `15` | minutes between the `memory` lines in the log; `0` for none |
| | `OVERSEER_MEMORY_GROWTH_MB` | `200` | how far a request grows the heap to have a `memory` line printed after it; `0` for every request |
| | `OVERSEER_DEPENDENCY_GRAPH` | on | `0` stops recording what each value was worked out from: a first open is a little quicker, every later one much slower |
| | `TZ` | `UTC` | the time zone a document's dates are read in |
| | `PUID`, `PGID` | the image's own user | in the image: the user and group the server runs as, and its folders are given to; never `0` |

For diagnosis only: `OVERSEER_PROFILE` prints what formulas cost, and `OVERSEER_DEBUG_SNAPSHOTS`
reports each node that a save is about to reformat.
