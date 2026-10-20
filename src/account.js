import { invoke } from '@tauri-apps/api/core'

/**
 * Who is looking, on a server that keeps accounts: what the page shows them and lets them do.
 *
 * The server answers `access` with who is asking - a person, a stranger, or the token (`anyone`) -
 * and whether the open document is theirs to change. From that the page draws a document shared
 * only to be read as read-only, keeps from a stranger what the server would refuse them, and gives
 * a person their profile: their name and password, their sharing document, signing out, and for an
 * admin the links that invite someone or set a new password.
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
        this.navigate = (url) => window.location.assign(url)
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
            return new URLSearchParams(window.location.search).get('doc')
        } catch (_) {
            return null
        }
    }

    wire() {
        const on = (id, what) => {
            const el = document.getElementById(id)
            if (el) el.addEventListener('click', what)
        }
        on('account-save-name', () => this.saveName())
        on('account-save-password', () => this.savePassword())
        on('account-sharing', () => this.openSharing())
        on('account-sign-out', () => this.signOut())
        on('account-invite', () => this.invite())
        on('account-reset', () => this.invite(this.valueOf('account-reset-username')))
        on('account-copy', () => this.copy())
        const signIn = document.getElementById('drawer-sign-in')
        if (signIn) {
            const here = window.location.pathname + window.location.search
            signIn.setAttribute('href', '/signin?next=' + encodeURIComponent(here))
        }
    }

    paint() {
        const section = document.getElementById('drawer-account')
        if (section) section.hidden = !this.person
        const who = document.getElementById('account-who')
        if (who) who.textContent = this.person ? this.person.name || this.person.username : ''
        const name = document.getElementById('account-name')
        if (name && this.person) name.value = this.person.name || ''
        const admin = document.getElementById('account-admin')
        if (admin) admin.hidden = !(this.person && this.person.admin)
        const signIn = document.getElementById('drawer-sign-in')
        if (signIn) signIn.hidden = this.asker !== 'stranger'
        const all = document.getElementById('drawer-all-documents')
        if (all) all.hidden = this.asker === 'stranger'
    }

    valueOf(id) {
        const el = document.getElementById(id)
        return el ? String(el.value || '').trim() : ''
    }

    /// Say how a change went, beside the form it came from.
    say(id, message, failed = false) {
        const el = document.getElementById(id)
        if (!el) return
        el.textContent = message
        el.classList.toggle('error', failed)
    }

    /// A call to the server's account routes, which are no command of the page's.
    async post(url, body) {
        const response = await fetch(url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body || {}),
        })
        const answer = await response.json().catch(() => null)
        if (!response.ok) throw new Error((answer && answer.error) || `failed (${response.status})`)
        return answer
    }

    async saveName() {
        const name = this.valueOf('account-name')
        if (!name) return this.say('account-name-said', 'A name cannot be empty.', true)
        try {
            this.person = await this.post('/account/me', { name })
            this.paint()
            this.say('account-name-said', 'Saved.')
        } catch (e) {
            this.say('account-name-said', e.message, true)
        }
    }

    async savePassword() {
        const current = document.getElementById('account-current')
        const fresh = document.getElementById('account-password')
        if (!current || !fresh) return
        try {
            await this.post('/account/me', { password: fresh.value, current: current.value })
            current.value = ''
            fresh.value = ''
            this.say('account-password-said', 'Changed.')
        } catch (e) {
            this.say('account-password-said', e.message, true)
        }
    }

    /// The person's own sharing document, made the first time it is asked for.
    async openSharing() {
        try {
            const made = await this.post('/account/sharing')
            this.navigate('/doc/' + String(made.document).split('/').map(encodeURIComponent).join('/'))
        } catch (e) {
            console.warn('[Overseer] the sharing document could not be opened', e)
            if (this.app && this.app.drawer) this.app.drawer.notice('The sharing document could not be opened.')
        }
    }

    async signOut() {
        try {
            await fetch('/signout', { method: 'POST' })
        } catch (_) { /* the sign-in page is where this goes either way */ }
        this.navigate('/signin')
    }

    /// A link that lets someone join, or with a username one that sets their new password.
    async invite(reset) {
        try {
            const made = await this.post('/account/invite', reset ? { reset } : {})
            const link = document.getElementById('account-link')
            const shown = document.getElementById('account-link-row')
            if (link) link.value = window.location.origin + made.link
            if (shown) shown.hidden = false
            this.say('account-invite-said', reset
                ? `A link that sets a new password for ${reset}, good for 7 days:`
                : 'A link to join, good for 7 days and for one person:')
        } catch (e) {
            this.say('account-invite-said', e.message, true)
        }
    }

    async copy() {
        const link = document.getElementById('account-link')
        if (!link || !link.value) return
        try {
            await navigator.clipboard.writeText(link.value)
            this.say('account-invite-said', 'Copied.')
        } catch (_) {
            link.select()
        }
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
