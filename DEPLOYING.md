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

Every machine on the network can then open `http://<server>:4747/` and read and change the
documents. `--open` is what allows that: without a token the server refuses to listen anywhere
but `127.0.0.1` unless it is asked to by name, and it prints a warning when it is. Leave `--open`
out and set `OVERSEER_TOKEN` instead to require a token on the network too.

Or with the image, from the root of this repository:

```bash
docker build -t overseer .
docker run -d -p 4747:4747 -v /srv/documents:/data -e OVERSEER_OPEN=1 overseer
```

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
     beside it, for a program such as a bot; see [API.md](API.md).

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
- `OVERSEER_DISK_CACHE` - the folder for what is let go, by default `overseer-cache` in the
  system's temporary folder. Set it empty to keep nothing on disk.
- `OVERSEER_DISK_CACHE_MB` - how much that folder may hold, 512 by default; `0` turns it off.
- `MALLOC_ARENA_MAX` - how many heaps the C allocator keeps. The image sets `2`: by default it
  keeps one per busy thread and never gives them back, and a server that holds little at rest
  can sit on gigabytes. Without it, set it yourself when running the binary on Linux.

The server also hands free memory back to the system after every request.

Stopping the whole service when nobody uses it - Railway's serverless mode, Fly's auto-stop,
Render's free tier - is a setting of the host, not of Overseer, and it is safe to turn on: the
documents are on the volume, nothing else is kept between runs, and the first request after a
wake-up works its documents out again.

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
| | `OVERSEER_LIBRARY` | the checkout it was built from, else `library` beside the binary (`/app/library` in the image) | the templates shipped in `library/` |
| | `OVERSEER_CACHE_MB` | `64` | see above |
| | `OVERSEER_DISK_CACHE` | system temp folder | see above |
| | `OVERSEER_DISK_CACHE_MB` | `512` | see above |
| | `OVERSEER_DEPENDENCY_GRAPH` | on | `0` stops recording what each value was worked out from: a first open is a little quicker, every later one much slower |
| | `TZ` | `UTC` | the time zone a document's dates are read in |

For diagnosis only: `OVERSEER_PROFILE` prints what formulas cost, and `OVERSEER_DEBUG_SNAPSHOTS`
reports each node that a save is about to reformat.
