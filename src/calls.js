import { invoke } from '@tauri-apps/api/core'

/**
 * What a document's `call` asks of the page: one operation of a fixed set, run once the press is
 * answered.
 *
 * Opening a document, changing a password, signing out are the host's to do, never the
 * document's - so a press only names them, with its arguments worked out, and this runs them.
 * Each does what the drawer's own buttons do, through the same commands and the same account
 * routes, and the server checks each request as it checks one made by hand: a document can ask
 * for nothing its viewer could not already do.
 *
 * An operation answers with a sentence, a key or a link, or with nothing. A failure answers with
 * its sentence. Either goes into the call's guarded `into` field when it has one; a failure with
 * nowhere to go is said on the status line.
 */

/// The account routes, which are no command of the page's: refused with the server's sentence.
export async function request(method, url, body) {
    const options = { method, headers: {} }
    if (body !== undefined) {
        options.headers['Content-Type'] = 'application/json'
        options.body = JSON.stringify(body)
    }
    const response = await fetch(url, options)
    const answer = await response.json().catch(() => null)
    if (!response.ok) throw new Error((answer && answer.error) || `failed (${response.status})`)
    return answer
}

/// An operation this host does not have: the desktop keeps no accounts, a browser opens no file
/// from the disk.
export class NotHere extends Error {
    constructor(name) {
        super(`"${name}" is not available here`)
    }
}

const text = (value) => (value === undefined || value === null ? '' : String(value))

/// The operations, by name, each given the call's arguments and the app. Where one needs a host,
/// `browser` or `desktop` says which.
export const OPERATIONS = {
    async open({ path }, app) {
        const place = text(path)
        if (!place) throw new Error('Nothing to open.')
        if (app.drawer) return void await app.drawer.open(place)
        if (inBrowser()) return void navigate('/doc/' + place.split('/').map(encodeURIComponent).join('/'))
        await app.loadFile(place)
    },
    async undo(_, app) {
        await app.undoLastChange()
    },
    async keep({ path }, app) {
        const place = text(path) || (app.currentFile ? String(app.currentFile) : '')
        if (!place) throw new Error('No document to keep.')
        const places = await invoke('menu_keep', { place })
        listChanged(app, places)
    },
    async drop({ path }, app) {
        const place = text(path)
        if (!place) throw new Error('No document to take off the list.')
        const places = await invoke('menu_drop', { place })
        listChanged(app, places)
    },
    async look({ theme, mode, week_start }, app) {
        const choice = {}
        if (text(theme)) choice.theme = text(theme)
        if (text(mode)) choice.mode = text(mode)
        if (text(week_start)) choice.weekStart = text(week_start)
        const settings = await invoke('settings_choose', choice)
        if (app.drawer) {
            app.drawer.settings = settings
            app.drawer.paintSettings()
            await app.drawer.redraw()
        } else if (app.currentFile && typeof app.loadFile === 'function') {
            await app.loadFile(app.currentFile)
        }
    },
    save_profile: browser(async ({ name, colour, icon, shape }, app) => {
        if (!text(name).trim()) throw new Error('A name cannot be empty.')
        const person = await request('POST', '/account/me', {
            name: text(name).trim(), colour: text(colour), icon: text(icon), shape: text(shape)
        })
        if (app.account) {
            app.account.person = person
            app.account.paint()
        }
        return 'Saved.'
    }),
    change_password: browser(async ({ current, password }) => {
        await request('POST', '/account/me', { password: text(password), current: text(current) })
        return 'Changed.'
    }),
    make_key: browser(async ({ name }) => {
        if (!text(name).trim()) throw new Error('A key needs a name.')
        const made = await request('POST', '/account/keys', { name: text(name).trim() })
        return made.key
    }),
    drop_key: browser(async ({ name }) => {
        await request('DELETE', '/account/keys/' + encodeURIComponent(text(name)))
        return `Revoked "${text(name)}".`
    }),
    invite: browser(async () => {
        const made = await request('POST', '/account/invite', {})
        return window.location.origin + made.link
    }),
    reset_password: browser(async ({ person }) => {
        if (!text(person)) throw new Error('Whose password?')
        const made = await request('POST', '/account/invite', { reset: text(person) })
        return window.location.origin + made.link
    }),
    sign_out: browser(async () => {
        try {
            await fetch('/signout', { method: 'POST' })
        } catch (_) { /* the sign-in page is where this goes either way */ }
        navigate('/signin')
    }),
    async copy({ text: what }) {
        await navigator.clipboard.writeText(text(what))
        return 'Copied.'
    },
    async fold_drawer({ folded }, app) {
        if (!app.drawer) return
        app.drawer.fold(folded === undefined || folded === null ? !app.drawer.isFolded() : !!folded)
    },
    open_file: desktop(async (_, app) => { await app.openFile() }),
    new_file: desktop(async (_, app) => { await app.newFile() }),
}

function inBrowser() {
    return typeof window !== 'undefined' && window.__OVERSEER_HOST__ === 'browser'
}

function navigate(url) {
    window.location.assign(url)
}

function browser(operation) {
    operation.host = 'browser'
    return operation
}

function desktop(operation) {
    operation.host = 'desktop'
    return operation
}

/// The drawer shows the list of documents, and the window's title may come from it.
function listChanged(app, places) {
    if (!app.drawer) return
    if (Array.isArray(places)) app.drawer.places = places
    app.drawer.paint()
    app.drawer.named()
}

/**
 * Run one operation. Answers what it gave - text, or null for nothing - and whether it failed.
 * Never throws: a call that fails is said, and the calls after it still run.
 */
export async function runCall(call, app, operations = OPERATIONS) {
    const name = call && call.name
    const operation = Object.prototype.hasOwnProperty.call(operations, name) ? operations[name] : null
    try {
        if (!operation) throw new NotHere(name)
        if (operation.host && operation.host !== (inBrowser() ? 'browser' : 'desktop')) throw new NotHere(name)
        const answer = await operation((call && call.args) || {}, app)
        return { answer: answer === undefined || answer === null ? null : String(answer), failed: false }
    } catch (e) {
        if (!(e instanceof NotHere)) console.warn(`[Overseer] the call "${name}" failed`, e)
        return { answer: (e && e.message) || `"${name}" failed`, failed: true }
    }
}
