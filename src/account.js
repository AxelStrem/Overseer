import { invoke } from '@tauri-apps/api/core'

/**
 * Who is looking, on a server that keeps accounts: what the page shows them and lets them do.
 *
 * The server answers `access` with who is asking - a person, a stranger, or the token (`anyone`) -
 * and whether the open document is theirs to change. From that the page draws a document shared
 * only to be read as read-only, and offers a stranger the way in. What a person does with their
 * account - their profile, password, keys, sharing, the links an admin makes - is the drawer
 * document's (`library/drawer/drawer.os`), through its `call`s.
 *
 * With no document open it lists what the person may open: their own, what others share with them,
 * and what is public.
 *
 * The desktop app has no accounts, and a server without them answers as the token: both are left
 * exactly as they were.
 */
export class Account {
    constructor(app) {
        this.app = app
        this.inBrowser = typeof window !== 'undefined' && window.__OVERSEER_HOST__ === 'browser'
        this.asker = 'anyone'
        this.person = null
        this.can = null
        // What loading a document waits for, so that one shared only to be read is never drawn
        // editable first.
        this.ready = Promise.resolve()
    }

    start() {
        if (!this.inBrowser) return this.ready
        this.ready = this.ask()
        this.wire()
        return this.ready
    }

    async ask() {
        let answer = null
        try {
            answer = await invoke('access')
        } catch (e) {
            // A server from before accounts, or a document that cannot be read: drawn as before.
            console.warn('[Overseer] could not tell who is looking', e)
        }
        this.take(answer)
    }

    /// What the server said, applied to the page.
    take(answer) {
        this.asker = (answer && answer.asker) || 'anyone'
        this.person = (answer && answer.person) || null
        this.can = (answer && answer.can) || null
        if (this.app && this.app.renderer) this.app.renderer.readOnly = this.can === 'read'
        if (this.can === 'read' && this.app && this.app.currentFile && typeof this.app.updateTitle === 'function') {
            this.app.updateTitle()
        }
        const body = document.body
        body.classList.toggle('viewer-stranger', this.asker === 'stranger')
        body.classList.toggle('viewer-person', !!this.person)
        body.classList.toggle('viewer-admin', !!(this.person && this.person.admin))
        body.classList.toggle('document-read-only', this.can === 'read')
        this.paint()
        if (!this.documentName()) this.list()
    }

    /// The document this page is showing, as the bridge read it from the address.
    documentName() {
        try {
            const path = decodeURIComponent(window.location.pathname || '')
            if (path.startsWith('/doc/')) return path.slice('/doc/'.length)
            if (path.startsWith('/examples/')) return path.slice('/examples/'.length)
            return new URLSearchParams(window.location.search).get('doc')
        } catch (_) {
            return null
        }
    }

    /// The way in, for a stranger: back to this document once signed in.
    wire() {
        const signIn = document.getElementById('drawer-sign-in')
        if (signIn) {
            const here = window.location.pathname + window.location.search
            signIn.setAttribute('href', '/signin?next=' + encodeURIComponent(here))
        }
    }

    paint() {
        const signIn = document.getElementById('drawer-sign-in')
        if (signIn) signIn.hidden = this.asker !== 'stranger'
    }

    /// Everything the person may open, under who it is from, in place of the welcome screen.
    async list() {
        const target = document.getElementById('document-list')
        if (!target || this.asker === 'stranger') return
        let listing = null
        try {
            const response = await fetch('/documents', { headers: { Accept: 'application/json' } })
            if (response.ok) listing = await response.json()
        } catch (e) {
            console.warn('[Overseer] the documents could not be listed', e)
        }
        if (!listing) return
        target.textContent = ''
        for (const group of groupsOf(listing, this.person && this.person.username)) {
            target.appendChild(this.groupElement(group))
        }
        target.hidden = false
    }

    groupElement(group) {
        const section = document.createElement('section')
        section.className = 'document-group'
        const heading = document.createElement('h3')
        heading.textContent = group.title
        section.appendChild(heading)
        for (const part of group.parts) {
            if (part.title) {
                const sub = document.createElement('h4')
                sub.textContent = part.title
                section.appendChild(sub)
            }
            const list = document.createElement('ul')
            for (const doc of part.documents) {
                const item = document.createElement('li')
                const link = document.createElement('a')
                link.href = '/doc/' + doc.name.split('/').map(encodeURIComponent).join('/')
                link.textContent = doc.shown
                item.appendChild(link)
                list.appendChild(item)
            }
            section.appendChild(list)
        }
        return section
    }
}

/**
 * What the documents a server lists are, under the headings the page shows them by: the person's
 * own, without their folder; what others share with them, under each owner, by full name; and
 * what is public and in neither. A server without accounts gives one list of everything.
 */
export function groupsOf(listing, me) {
    const entry = (name, shown) => ({ name, shown: shown || name })
    const documents = Array.isArray(listing && listing.documents) ? listing.documents : []
    const mine = Array.isArray(listing && listing.mine) ? listing.mine : null
    if (!mine) {
        return documents.length ? [{ title: 'Documents', parts: [{ documents: documents.map((d) => entry(d)) }] }] : []
    }
    const own = (name) => (me && name.startsWith(me + '/') ? name.slice(me.length + 1) : name)
    const groups = []
    if (mine.length) groups.push({ title: 'Mine', parts: [{ documents: mine.map((d) => entry(d, own(d))) }] })
    const shared = listing.shared && typeof listing.shared === 'object' ? listing.shared : {}
    const seen = new Set(mine)
    const owners = Object.keys(shared).filter((o) => Array.isArray(shared[o]) && shared[o].length).sort()
    if (owners.length) {
        groups.push({
            title: 'Shared with me',
            parts: owners.map((owner) => {
                for (const d of shared[owner]) seen.add(d)
                return { title: owner, documents: shared[owner].map((d) => entry(d)) }
            }),
        })
    }
    const unseen = (Array.isArray(listing.public) ? listing.public : []).filter((d) => !seen.has(d))
    if (unseen.length) groups.push({ title: 'Public', parts: [{ documents: unseen.map((d) => entry(d)) }] })
    return groups
}
