// See a document as the page draws it, and try what it does, without touching the file.
//
//   node scripts/preview.mjs <document.os> [--root DIR] [--script steps.mjs] [--out DIR]
//                            [--width 1500] [--height 900] [--mobile] [--keep]
//
// The document's folder (or --root, a folder above it, when the document reaches files outside
// its own) is copied to a temporary directory, and the release server serves the copy with the
// built page; a headless Chrome or Edge opens it. Nothing pressed reaches the real file, and the
// copy is deleted afterwards. Build both first - `npm run build`, and in src-tauri
// `cargo build --release --features server --bin overseer-server`.
//
// Without --script it saves one screenshot of the whole document and prints what went wrong
// while drawing: templates and mounts that could not be had, formulas that failed, what the page
// threw. With --script it runs the steps in that file, the body of an async function given:
//
//   shot(name, { of, full })   a screenshot, its path printed: the whole document, one node
//                              (of: address), or only what the window shows (full: false)
//   press(at), type(text), key('Enter')
//                              a click (a tap with --mobile), typing, a key - as a person's
//   pick(at, option), tab(name)
//                              an option of an enum or tags field; a tab, by name or label
//   text(at), drawn(at), node(at, depth)
//                              what the page shows; whether it is seen; the node as the page
//                              holds it, value and parameters
//   changes(), fileText()      what the file gained and lost since the start; the file now
//   problems(), log(...), wait(ms), settle(), resize({ width, height }), evaluate(fn, ...args)
//
// Each step waits for the page to settle - its writes landed and redrawn - before the next. A
// node is named by its address, as in a formula: "project/Items/[parser]/stage", where [key]
// is the entry with that key and [0] or [-1] one by its place - or by a CSS selector, as
// { css: '.tab-button' }. See preview_example.mjs. Screenshots go to --out, or AGENT_SCRATCH,
// or beside the script; a long page read whole is shrunk, so look at its parts with `of`.
//
// --keep leaves the server running, and prints its address, for a person to open the copy.
// CHROME names a Chromium browser if Chrome and Edge are not in the usual places.
import { spawn } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { basename, dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SERVER = join(REPO, 'src-tauri', 'target', 'release', process.platform === 'win32' ? 'overseer-server.exe' : 'overseer-server')
const DIST = join(REPO, 'dist')
// Chrome cannot draw a picture much taller than this; a longer document is cut, and says so.
const TALLEST = 15000

function parseArgs(argv) {
    const args = { width: 1500, height: 900 }
    const rest = []
    for (let i = 0; i < argv.length; i++) {
        const a = argv[i]
        if (a === '--mobile') args.mobile = true
        else if (a === '--keep') args.keep = true
        else if (['--root', '--script', '--out', '--width', '--height'].includes(a)) args[a.slice(2)] = argv[++i]
        else if (a === '--help' || a === '-h') args.help = true
        else if (a.startsWith('--')) throw new Error(`no option ${a} - see --help`)
        else rest.push(a)
    }
    args.document = rest[0]
    if (args.mobile) { args.width = 390; args.height = 844 }
    args.width = Number(args.width)
    args.height = Number(args.height)
    return args
}

function usage() {
    const text = readFileSync(fileURLToPath(import.meta.url), 'utf-8')
    console.log(text.split('\nimport ')[0].replace(/^\/\/ ?/gm, ''))
}

function newestIn(dir) {
    let newest = 0
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name)
        newest = Math.max(newest, entry.isDirectory() ? newestIn(path) : statSync(path).mtimeMs)
    }
    return newest
}

async function freePort() {
    return new Promise((done) => {
        const probe = createServer()
        probe.listen(0, '127.0.0.1', () => {
            const { port } = probe.address()
            probe.close(() => done(port))
        })
    })
}

const wait = (ms) => new Promise((r) => setTimeout(r, ms))

async function startServer(root, port) {
    const server = spawn(SERVER, ['--root', root, '--port', String(port), '--frontend', DIST], { stdio: 'ignore' })
    for (let i = 0; i < 150; i++) {
        try {
            await fetch(`http://127.0.0.1:${port}/`)
            return server
        } catch (_) {
            await wait(200)
        }
    }
    server.kill()
    throw new Error('the server did not start')
}

async function startBrowser(profile) {
    const chrome = [
        process.env.CHROME,
        'C:/Program Files/Google/Chrome/Application/chrome.exe',
        'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
        'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
        'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
        '/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser',
        '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    ].find((p) => p && existsSync(p))
    if (!chrome) throw new Error('no Chrome or Edge found; set CHROME to a Chromium browser')
    const browser = spawn(chrome, [
        '--headless=new', '--disable-gpu', '--hide-scrollbars', '--remote-debugging-port=0',
        '--no-first-run', '--no-default-browser-check', `--user-data-dir=${profile}`, 'about:blank',
    ])
    const url = await new Promise((done, fail) => {
        let seen = ''
        browser.stderr.on('data', (d) => {
            seen += d
            const m = seen.match(/DevTools listening on (ws:\S+)/)
            if (m) done(m[1])
        })
        setTimeout(() => fail(new Error('the browser gave no DevTools address')), 20000)
    })
    return { browser, debugPort: new URL(url).port }
}

/// One page over the DevTools protocol: commands, and the events the helpers listen for.
async function connect(debugPort) {
    const targets = await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json()
    const ws = new WebSocket(targets.find((t) => t.type === 'page').webSocketDebuggerUrl)
    await new Promise((r) => ws.addEventListener('open', r))
    let id = 0
    const pending = new Map()
    const listeners = []
    ws.addEventListener('message', (e) => {
        const msg = JSON.parse(e.data)
        if (msg.id && pending.has(msg.id)) {
            const { done, fail } = pending.get(msg.id)
            pending.delete(msg.id)
            if (msg.error) fail(new Error(`${msg.error.message}`))
            else done(msg.result)
        } else if (msg.method) {
            for (const listen of listeners) listen(msg.method, msg.params)
        }
    })
    const send = (method, params = {}) => new Promise((done, fail) => {
        const n = ++id
        pending.set(n, { done, fail })
        ws.send(JSON.stringify({ id: n, method, params }))
    })
    return { send, on: (listen) => listeners.push(listen), close: () => ws.close() }
}

// Run in the page: find a node's element by its address. Every drawn node carries the path it
// was drawn at as JSON in `data-path`, by the names of its nodes; an entry of a list is named by
// the page, so `[key]` in an address is matched against the entries' values in the document the
// page holds, as the language's own `[key]` is.
const IN_PAGE = String.raw`
window.__preview = {
    split(address) {
        if (Array.isArray(address)) return address.map(String)
        const parts = []
        let part = '', depth = 0
        for (const ch of String(address).replace(/^\/+/, '')) {
            if (ch === '[') depth++
            if (ch === ']') depth--
            if (ch === '/' && depth === 0) { parts.push(part); part = '' } else part += ch
        }
        if (part) parts.push(part)
        return parts
    },
    leaves(node) {
        const out = []
        const walk = (n) => {
            for (const c of (n && n.children) || []) {
                // As the engine has it: a div or tab with no name of its own is not on the path.
                if (!c.name || (c.is_hierarchy_transparent && c.name === c.node_type)) walk(c)
                else out.push(c)
            }
        }
        walk(node)
        return out
    },
    // The value as the page shows it.
    valueOf(node) {
        try { return window.app.renderer.getNodeValue(node) } catch (_) { return undefined }
    },
    // A parameter as written: { String: "x" } read as "x", and a formula as its text.
    bare(v) {
        if (v && typeof v === 'object') {
            const [k] = Object.keys(v)
            return k === 'Formula' ? '$(' + v[k] + ')' : this.bare(v[k])
        }
        return v
    },
    // The names the page drew the node under, from the address as a person writes it.
    pathOf(address) {
        const parts = this.split(address)
        const out = []
        let here = { children: window.app.currentDocument }
        for (const part of parts) {
            const keyed = part.match(/^\[(.*)\]$/)
            const named = this.leaves(here)
            let next
            if (keyed) {
                const key = keyed[1].replace(/^["']|["']$/g, '')
                next = named.find((e) => e.name === key)
                    || named.find((e) => this.leaves(e).some((f) => String(this.valueOf(f)) === key))
                if (!next && /^-?\d+$/.test(key)) next = named[Number(key) < 0 ? named.length + Number(key) : Number(key)]
            } else {
                next = named.find((e) => e.name === part)
            }
            if (!next) return { missing: part, under: out }
            out.push(next.name)
            here = next
        }
        return { path: out, node: here }
    },
    find(target) {
        if (target && target.css) {
            const all = Array.from(document.querySelectorAll(target.css))
            return { el: all.find((e) => e.getClientRects().length) || all[0], all }
        }
        const found = this.pathOf(target)
        if (found.missing !== undefined) return { why: 'no ' + JSON.stringify(found.missing) + ' under /' + found.under.join('/') }
        const wanted = JSON.stringify(found.path)
        const all = Array.from(document.querySelectorAll('[data-path]')).filter((e) => e.getAttribute('data-path') === wanted)
        if (!all.length) return { why: 'not drawn: /' + found.path.join('/'), node: found.node }
        // The outermost is the node itself; the rest are parts of it that carry its path too.
        const outer = all.filter((e) => !all.some((o) => o !== e && o.contains(e)))
        return { el: outer.find((e) => e.getClientRects().length) || outer[0], all, node: found.node }
    },
    rect(target, scroll) {
        const found = this.find(target)
        if (!found.el) return { why: found.why || 'nothing matches ' + JSON.stringify(target) }
        if (scroll) found.el.scrollIntoView({ block: 'center', inline: 'center' })
        const r = found.el.getBoundingClientRect()
        const out = { x: r.left, y: r.top, width: r.width, height: r.height,
                      pageX: r.left + window.scrollX, pageY: r.top + window.scrollY }
        // Where a press lands: the middle if the element is on top there, or else the first
        // point of a grid over it that is - and if none is, what lies over it, as a person's
        // press would find.
        const points = [[0.5, 0.5]]
        for (const fy of [0.25, 0.5, 0.75]) for (const fx of [0.15, 0.3, 0.5, 0.7, 0.85]) points.push([fx, fy])
        for (const [fx, fy] of points) {
            const x = r.left + r.width * fx, y = r.top + r.height * fy
            const top = document.elementFromPoint(x, y)
            if (top && (top === found.el || found.el.contains(top))) return { ...out, pressX: x, pressY: y }
        }
        const over = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2)
        out.covered = over ? over.tagName.toLowerCase() + (over.className ? '.' + String(over.className).trim().split(/\s+/).join('.') : '') : 'nothing on screen'
        return out
    },
    problems() {
        const out = []
        const status = document.getElementById('status-message')
        if (status && (status.classList.contains('error') || status.classList.contains('warning'))) out.push('status: ' + status.textContent.trim())
        const screen = document.getElementById('error-screen')
        if (screen && screen.classList.contains('active')) out.push('error screen: ' + (document.getElementById('error-message') || screen).textContent.trim())
        // Read from the document rather than the page, so that what is hidden or folded away is
        // found too. A formula that fails is worked out as this text, in a value or a parameter.
        const failed = 'invalid formula error'
        // A template's own formulas have no entry to read from, and fail there by design. A
        // template is known by its instances, which carry its name as their type.
        const kinds = new Set()
        const gather = (node) => {
            kinds.add(node.node_type)
            const entry = String(this.bare((node.parameters || {}).entry) ?? '').match(/<(?:[^>]*\/)?([^>/]+)>/)
            if (entry) kinds.add(entry[1])
            for (const c of node.children || []) gather(c)
        }
        for (const node of window.app.currentDocument || []) gather(node)
        const walk = (node, at) => {
            if (node.name && node.node_type !== node.name && kinds.has(node.name)) return
            const params = node.parameters || {}
            for (const key of ['_inherit_error', '_mount_error', '_parse_error']) {
                if (params[key] !== undefined) out.push(at + ': ' + this.bare(params[key]))
            }
            if (this.bare(params._computed_value) === failed || (params.value && params.value.Formula && this.valueOf(node) === failed)) {
                out.push(at + ': ' + failed + ' in ' + this.bare(params.value))
            }
            for (const [key, v] of Object.entries(params)) {
                const name = key.replace(/^_computed_/, '')
                // A lambda - sort_by=$(|x| ...) - and a plot's source list are no value of their
                // own: what uses them reads them, and they fail when worked out as one.
                const lambda = /^\$\(\s*\|/.test(String(this.bare(params[name]) ?? '')) || name === 'source'
                if (key.startsWith('_computed_') && name !== 'value' && !lambda && this.bare(v) === failed) {
                    out.push(at + ': ' + failed + ' in ' + name + '=' + (this.bare(params[name]) ?? '?'))
                }
            }
            const named = this.leaves(node)
            const listed = node.node_type === 'list'
            for (const child of node.children || []) {
                const i = named.indexOf(child)
                const step = i < 0 ? '' : '/' + (listed ? '[' + i + ']' : child.name)
                walk(child, at + step)
            }
        }
        for (const node of window.app.currentDocument || []) walk(node, (node.name && node.name !== node.node_type) ? '/' + node.name : '')
        return [...new Set(out)]
    },
    // A node to read in a log: its name, type, value and the parameters the document gave it -
    // not the ones the page works out for itself, which start with an underscore.
    plain(node, depth) {
        const out = { name: node.name, type: node.node_type }
        const value = this.valueOf(node)
        // A container's value is the page's own id for it.
        if (value !== undefined && value !== null && value !== '' && !/^uid_/.test(value)) out.value = value
        const params = {}
        for (const [k, v] of Object.entries(node.parameters || {})) if (!k.startsWith('_') && k !== 'value' && !(node.parameters['_inherited_' + k])) params[k] = this.bare(v)
        if (Object.keys(params).length) out.parameters = params
        const children = node.children || []
        if (children.length) out.children = depth > 0 ? children.map((c) => this.plain(c, depth - 1)) : children.length + ' children'
        return out
    },
}
`

/// The lines that differ between two texts, in order: `@ 12` before each place that changed,
/// then `- ` what went and `+ ` what came. The part both share at the start and the end is
/// skipped first, so a change to a long document compares only the few lines around it.
function diffLines(before, after) {
    const a = String(before || '').split(/\r?\n/)
    const b = String(after || '').split(/\r?\n/)
    let start = 0
    while (start < a.length && start < b.length && a[start] === b[start]) start++
    let end = 0
    while (end < a.length - start && end < b.length - start && a[a.length - 1 - end] === b[b.length - 1 - end]) end++
    const x = a.slice(start, a.length - end)
    const y = b.slice(start, b.length - end)
    if (x.length * y.length > 4e6) return [`@ ${start + 1}`, ...x.map((l) => '- ' + l), ...y.map((l) => '+ ' + l)]
    // The longest run the two share, by the usual table, walked from its corner.
    const table = Array.from({ length: x.length + 1 }, () => new Uint32Array(y.length + 1))
    for (let i = x.length - 1; i >= 0; i--) {
        for (let j = y.length - 1; j >= 0; j--) {
            table[i][j] = x[i] === y[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1])
        }
    }
    const out = []
    let i = 0, j = 0, open = false
    while (i < x.length || j < y.length) {
        if (i < x.length && j < y.length && x[i] === y[j]) {
            i++; j++; open = false
            continue
        }
        if (!open) { out.push(`@ ${start + j + 1}`); open = true }
        if (j >= y.length || (i < x.length && table[i + 1][j] >= table[i][j + 1])) out.push('- ' + x[i++])
        else out.push('+ ' + y[j++])
    }
    return out
}

async function main() {
    const args = parseArgs(process.argv.slice(2))
    if (args.help || !args.document) return usage()

    const file = resolve(args.document)
    if (!existsSync(file)) throw new Error(`no document at ${file}`)
    const root = resolve(args.root || dirname(file))
    const doc = relative(root, file).split('\\').join('/')
    if (doc.startsWith('..')) throw new Error('--root has to be a folder the document is in')
    for (const [what, path] of [['server', SERVER], ['built page', join(DIST, 'index.html')]]) {
        if (!existsSync(path)) throw new Error(`no ${what} at ${path}: build it first, as the top of this file says`)
    }
    // A page or server older than its source shows what the code was, not what it is.
    if (newestIn(join(REPO, 'src')) > statSync(join(DIST, 'index.html')).mtimeMs) console.log('warning: src/ is newer than dist/ - run `npm run build`')
    if (newestIn(join(REPO, 'src-tauri', 'src')) > statSync(SERVER).mtimeMs) console.log('warning: src-tauri/src is newer than the server - rebuild it')

    const out = resolve(args.out || process.env.AGENT_SCRATCH || (args.script ? dirname(resolve(args.script)) : '.'))
    mkdirSync(out, { recursive: true })
    const work = mkdtempSync(join(tmpdir(), 'preview_'))
    const copy = join(work, 'documents')
    cpSync(root, copy, { recursive: true, filter: (from) => !/[\\/](\.git|node_modules|target)$/.test(from) })

    const port = await freePort()
    const server = await startServer(copy, port)
    const base = `http://127.0.0.1:${port}`
    let browser
    try {
        const started = await startBrowser(join(work, 'profile'))
        browser = started.browser
        const page = await connect(started.debugPort)
        const exceptions = []
        // The requests still open, by id. The page's watch on the file is a stream that stays open
        // for as long as the page does, so it is left out - else nothing would ever settle.
        const open = new Set()
        let lastChange = Date.now()
        page.on((method, params) => {
            if (method === 'Runtime.exceptionThrown') {
                const d = params.exceptionDetails
                exceptions.push('exception: ' + ((d.exception && d.exception.description) || d.text).split('\n').slice(0, 3).join(' | '))
            } else if (method === 'Runtime.consoleAPICalled' && params.type === 'error') {
                exceptions.push('console error: ' + params.args.map((a) => a.value ?? a.description ?? '').join(' ').slice(0, 400))
            } else if (method === 'Network.requestWillBeSent') {
                if (params.type === 'EventSource' || /\/v1\/watch/.test(params.request.url)) return
                open.add(params.requestId)
                lastChange = Date.now()
            } else if (method === 'Network.loadingFinished' || method === 'Network.loadingFailed') {
                if (!open.delete(params.requestId)) return
                lastChange = Date.now()
            }
        })
        await page.send('Page.enable')
        await page.send('Runtime.enable')
        await page.send('Network.enable')
        await page.send('Emulation.setFocusEmulationEnabled', { enabled: true })
        const metrics = { width: args.width, height: args.height, deviceScaleFactor: 1, mobile: !!args.mobile }
        await page.send('Emulation.setDeviceMetricsOverride', metrics)
        if (args.mobile) await page.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 })
        await page.send('Page.addScriptToEvaluateOnNewDocument', { source: IN_PAGE })
        await page.send('Page.navigate', { url: `${base}/doc/${doc}` })

        const evaluate = async (fn, ...params) => {
            const expression = typeof fn === 'function' ? `(${fn})(...${JSON.stringify(params)})` : String(fn)
            const r = await page.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
            if (r.exceptionDetails) throw new Error('in the page: ' + ((r.exceptionDetails.exception && r.exceptionDetails.exception.description) || r.exceptionDetails.text))
            return r.result.value
        }
        // Quiet: no request open, and the page unchanged, for a while - so a write has landed
        // and what it redrew is drawn.
        const settle = async (quiet = 400, most = 10000) => {
            await evaluate(() => {
                window.__previewChanged = Date.now()
                if (!window.__previewWatch) {
                    window.__previewWatch = new MutationObserver(() => { window.__previewChanged = Date.now() })
                    window.__previewWatch.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true })
                }
            })
            const until = Date.now() + most
            while (Date.now() < until) {
                await wait(100)
                const changed = await evaluate('window.__previewChanged')
                if (open.size === 0 && Date.now() - Math.max(changed, lastChange) >= quiet) return
            }
        }
        for (let i = 0; i < 300; i++) {
            if (await evaluate('!!(window.app && Array.isArray(window.app.currentDocument)) || !!document.querySelector("#error-screen.active")').catch(() => false)) break
            await wait(100)
        }
        await settle()

        const lines = []
        const log = (...parts) => {
            // A list of lines - what changes() gives - is printed as lines, not as JSON.
            const line = parts.map((p) => typeof p === 'string' ? p
                : Array.isArray(p) && p.every((l) => typeof l === 'string') ? '\n  ' + p.join('\n  ')
                : JSON.stringify(p, null, 1)).join(' ')
            lines.push(line)
            console.log(line)
        }
        const rectOf = async (target, scroll = true) => {
            const r = await evaluate((t, s) => window.__preview.rect(t, s), target, scroll)
            if (r.why) throw new Error(r.why)
            return r
        }
        const mouse = async (x, y, type, extra = {}) => page.send('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1, ...extra })
        const fileText = async (name = doc) => {
            const res = await fetch(`${base}/api/load_overseer_file`, {
                method: 'POST', headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ document: name, session: 'preview', args: { path: name } }),
            })
            return (await res.json()).result
        }
        const original = await fileText()
        let shots = 0

        const helpers = {
            log,
            wait,
            settle,
            evaluate,
            /// A screenshot, saved and its path printed: the whole document, what the window
            /// shows (`{ full: false }`), or one node (`{ of: address }`).
            async shot(name = 'page', { of, full = true, margin = 8 } = {}) {
                await settle()
                const path = join(out, `${String(++shots).padStart(2, '0')}-${name.replace(/[^\w.-]+/g, '_')}.png`)
                let shot
                let cut = false
                if (of) {
                    const r = await rectOf(of)
                    const clip = { x: Math.max(0, r.pageX - margin), y: Math.max(0, r.pageY - margin), width: r.width + 2 * margin, height: Math.min(TALLEST, r.height + 2 * margin), scale: 1 }
                    shot = await page.send('Page.captureScreenshot', { format: 'png', clip, captureBeyondViewport: true })
                } else if (full) {
                    // The window grown to the document's height, rather than the picture taken
                    // beyond it, so that what is fixed to the window - the drawer - runs its length.
                    const height = await evaluate(() => document.documentElement.scrollHeight)
                    cut = height > TALLEST
                    await page.send('Emulation.setDeviceMetricsOverride', { ...metrics, height: Math.min(height, TALLEST) })
                    await settle(200)
                    shot = await page.send('Page.captureScreenshot', { format: 'png' })
                    await page.send('Emulation.setDeviceMetricsOverride', metrics)
                    await settle(200)
                } else {
                    shot = await page.send('Page.captureScreenshot', { format: 'png' })
                }
                writeFileSync(path, Buffer.from(shot.data, 'base64'))
                log(`screenshot: ${path}${cut ? ` (cut at ${TALLEST}px)` : ''}`)
                return path
            },
            /// A press, as a person's: the mouse moved there and clicked - or, in --mobile, a tap.
            async press(target, { double = false } = {}) {
                const r = await rectOf(target)
                if (r.covered) throw new Error(`${JSON.stringify(target)} cannot be pressed: ${r.covered} lies over it`)
                const x = r.pressX
                const y = r.pressY
                if (args.mobile) {
                    await page.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] })
                    await page.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] })
                } else {
                    await mouse(x, y, 'mouseMoved', { button: 'none' })
                    for (const count of double ? [1, 2] : [1]) {
                        await mouse(x, y, 'mousePressed', { clickCount: count })
                        await mouse(x, y, 'mouseReleased', { clickCount: count })
                    }
                }
                await settle()
            },
            async type(text) {
                await page.send('Input.insertText', { text })
                await settle(200)
            },
            async key(name) {
                const codes = { Enter: 13, Escape: 27, Tab: 9, Backspace: 8, Delete: 46, ArrowUp: 38, ArrowDown: 40, ArrowLeft: 37, ArrowRight: 39 }
                const code = codes[name]
                const base = { key: name, code: name, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code }
                await page.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...base, ...(name === 'Enter' ? { text: '\r' } : {}) })
                if (name === 'Enter') await page.send('Input.dispatchKeyEvent', { type: 'char', ...base, text: '\r' })
                await page.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base })
                await settle()
            },
            /// Pick an option of an enum or tags field: the add button of a tags field, or the
            /// chip of an enum, pressed, then the option.
            async pick(target, option) {
                const chip = await evaluate((t) => {
                    const found = window.__preview.find(t)
                    // One selector at a time: a list of them matches in document order, and a
                    // tags field draws the tags it holds before its add button.
                    const first = (...selectors) => selectors.map((s) => found.el.querySelector(s)).find(Boolean)
                    const chip = found.el && (first('.tag-add', '.enum-choice', '.tag-chip') || found.el)
                    if (!chip) return null
                    chip.setAttribute('data-preview-pick', '')
                    return true
                }, target)
                if (!chip) throw new Error('nothing to pick on at ' + JSON.stringify(target))
                await helpers.press({ css: '[data-preview-pick]' })
                await evaluate(() => document.querySelectorAll('[data-preview-pick]').forEach((e) => e.removeAttribute('data-preview-pick')))
                const there = await evaluate((o) => {
                    const all = Array.from(document.querySelectorAll('.tag-picker .tag-option'))
                    const hit = all.find((e) => e.textContent.trim() === o) || all.find((e) => e.textContent.trim().includes(o))
                    if (!hit) return all.map((e) => e.textContent.trim())
                    hit.setAttribute('data-preview-option', '')
                    return true
                }, option)
                if (there !== true) throw new Error(`no option ${JSON.stringify(option)}; the picker offers ${JSON.stringify(there)}`)
                await helpers.press({ css: '[data-preview-option]' })
            },
            /// Open a tab by its name or its label.
            async tab(name) {
                const there = await evaluate((n) => {
                    const all = Array.from(document.querySelectorAll('.tab-button'))
                    const hit = all.find((b) => b.dataset.tab === n) || all.find((b) => b.textContent.trim() === n)
                    if (!hit) return all.map((b) => b.dataset.tab || b.textContent.trim())
                    document.querySelectorAll('[data-preview-tab]').forEach((e) => e.removeAttribute('data-preview-tab'))
                    hit.setAttribute('data-preview-tab', '')
                    return true
                }, name)
                if (there !== true) throw new Error(`no tab ${JSON.stringify(name)}; there are ${JSON.stringify(there)}`)
                await helpers.press({ css: '[data-preview-tab]' })
            },
            /// The text the page shows for a node.
            async text(target) {
                return evaluate((t) => {
                    const found = window.__preview.find(t)
                    if (!found.el) throw new Error(found.why || 'nothing matches')
                    return found.el.innerText.trim()
                }, target)
            },
            /// Whether a node is drawn, and seen.
            async drawn(target) {
                return evaluate((t) => {
                    const found = window.__preview.find(t)
                    return !!(found.el && found.el.getClientRects().length)
                }, target)
            },
            /// The node as the page holds it, its value worked out, to the depth asked.
            async node(target, depth = 1) {
                return evaluate((t, d) => {
                    const found = window.__preview.pathOf(t)
                    if (found.missing !== undefined) throw new Error('no ' + JSON.stringify(found.missing) + ' under /' + found.under.join('/'))
                    return window.__preview.plain(found.node, d)
                }, target, depth)
            },
            fileText,
            /// What the file gained and lost since the start - what the presses really wrote -
            /// as the lines changed, each place headed by its line number in the file now.
            async changes(name = doc) {
                await settle()
                return diffLines(name === doc ? original : '', await fileText(name))
            },
            /// What went wrong while drawing: the status line, templates and mounts that could
            /// not be had, and what the page threw.
            async problems() {
                return [...await evaluate(() => window.__preview.problems()), ...exceptions]
            },
            /// Grow or shrink the window: { width, height }.
            async resize(size) {
                Object.assign(metrics, size)
                await page.send('Emulation.setDeviceMetricsOverride', metrics)
                await settle()
            },
            document: doc,
            root: copy,
        }

        if (args.script) {
            const body = readFileSync(resolve(args.script), 'utf-8')
            const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor
            const steps = new AsyncFunction(...Object.keys(helpers), body)
            try {
                await steps(...Object.values(helpers))
            } catch (e) {
                log('script error: ' + (e && e.message || e))
            }
        } else {
            await helpers.shot(basename(doc, '.os') + (args.mobile ? '-mobile' : ''))
        }
        const problems = await helpers.problems()
        log(problems.length ? 'problems:\n  ' + problems.join('\n  ') : 'problems: none')
        if (args.keep) {
            console.log(`serving the copy at ${base}/doc/${doc} - Ctrl+C to stop`)
            // Stopped by Ctrl+C, the copy is still deleted on the way out.
            await new Promise((done) => process.once('SIGINT', done))
        }
        page.close()
    } finally {
        if (browser) browser.kill()
        server.kill()
        await wait(500)
        rmSync(work, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 })
    }
}

main().then(() => process.exit(0), (e) => {
    console.log('preview error: ' + (e && e.message || e))
    process.exit(1)
})
