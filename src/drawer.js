import { invoke } from '@tauri-apps/api/core'

/**
 * The drawer: the documents kept to hand, and what there is to do with the one that is open.
 *
 * Documents live wherever they were made, so getting from one to another meant the file dialog
 * every time on the desktop and a bookmark per document in a browser. The drawer lists the ones
 * worth getting to quickly. The list is a document of its own - `menu.os`, see the backend's
 * `menu` module - so it is backed up with the rest, and renamed or reordered by opening it like
 * any other. The desktop app keeps one in its settings folder, naming files anywhere on the
 * computer; the server keeps one in its documents folder, naming documents under it.
 *
 * It also took over what the header and the status bar at the foot of the page held - the path,
 * Undo, opening and making a file, the status line - so that folded away it leaves the document
 * nearly the whole of the screen. On a narrow screen it lies over the page instead of beside it,
 * and starts folded.
 */
export class Drawer {
    constructor(app) {
        this.app = app
        this.places = []
        // The bridge says so when the page is running in a browser against the server; the
        // desktop app has no bridge. A document is a file there and a name here, and opening one
        // is loading it there and going to its address here.
        this.inBrowser = typeof window !== 'undefined' && window.__OVERSEER_HOST__ === 'browser'
        this.navigate = (url) => window.location.assign(url)
        this.element = document.getElementById('drawer')
    }

    /// Wire it up and read the list. Nothing to do on a page without one.
    start() {
        if (!this.element) return
        document.body.classList.toggle('in-browser', this.inBrowser)
        this.fold(this.remembered())
        const on = (id, what) => {
            const el = document.getElementById(id)
            if (el) el.addEventListener('click', what)
        }
        on('drawer-fold', () => this.fold(true))
        on('drawer-open', () => this.fold(false))
        on('drawer-backdrop', () => this.fold(true))
        on('drawer-keep', () => this.keep())
        on('drawer-edit-list', () => this.editList())
        // Ctrl+\, as in the tools this was modelled on. One listener for the page, whichever
        // drawer was started last - a second would toggle it straight back.
        if (window.__overseerDrawerKeys) document.removeEventListener('keydown', window.__overseerDrawerKeys)
        window.__overseerDrawerKeys = (event) => {
            if ((event.ctrlKey || event.metaKey) && event.key === '\\') {
                event.preventDefault()
                this.fold(!this.isFolded())
            }
        }
        document.addEventListener('keydown', window.__overseerDrawerKeys)
        return this.refresh()
    }

    isFolded() {
        return document.body.classList.contains('drawer-folded')
    }

    /// Folded or not, and remembered for the next page - per viewer, and only a convenience.
    fold(folded) {
        document.body.classList.toggle('drawer-folded', !!folded)
        try { localStorage.setItem('overseer.drawer.folded', folded ? '1' : '0') } catch (_) { /* not kept */ }
    }

    remembered() {
        try {
            const said = localStorage.getItem('overseer.drawer.folded')
            if (said === '1' || said === '0') return said === '1'
        } catch (_) { /* nothing remembered */ }
        // Open where there is room for it beside the page, folded where it would cover it.
        return typeof window !== 'undefined' && window.innerWidth < 768
    }

    /// Where the open document is, as the list names documents.
    current() {
        return this.app && this.app.currentFile ? String(this.app.currentFile) : null
    }

    async refresh() {
        try {
            const places = await invoke('menu_places')
            this.places = Array.isArray(places) ? places : []
        } catch (e) {
            console.warn('[Overseer] the list of documents could not be read', e)
            this.places = []
        }
        this.paint()
    }

    /// What the drawer calls a document: the name it was kept under, or its file's name.
    nameOf(place) {
        if (place.name) return place.name
        const file = String(place.path).split(/[\\/]/).pop() || String(place.path)
        return file.replace(/\.os$/, '')
    }

    /// What to call the open document when it is kept: its first tab's label, which is what it
    /// calls itself, or nothing, which leaves it to the file's name.
    nameForCurrent() {
        try {
            const doc = this.app.currentDocument
            const tab = (Array.isArray(doc) ? doc : []).find((n) => (n.node_type || n.type) === 'tab')
            const label = tab && this.app.renderer.getParameterValue(tab, 'label')
            return label ? String(label) : ''
        } catch (_) {
            return ''
        }
    }

    paint() {
        const list = document.getElementById('drawer-places')
        if (!list) return
        list.textContent = ''
        const here = this.current()
        if (this.places.length === 0) {
            const empty = document.createElement('li')
            empty.className = 'drawer-empty'
            empty.textContent = 'Nothing kept here yet.'
            list.appendChild(empty)
        }
        for (const place of this.places) {
            const item = document.createElement('li')
            item.className = 'drawer-place'
            const open = document.createElement('button')
            open.className = 'drawer-place-open'
            open.textContent = this.nameOf(place)
            open.title = place.path
            if (place.path === here) {
                item.classList.add('current')
                open.setAttribute('aria-current', 'page')
            }
            open.addEventListener('click', () => this.open(place.path))
            const drop = document.createElement('button')
            drop.className = 'drawer-place-drop'
            drop.textContent = '×'
            drop.title = 'Take off this list'
            drop.setAttribute('aria-label', `Take ${this.nameOf(place)} off this list`)
            drop.addEventListener('click', (event) => {
                event.stopPropagation()
                this.drop(place.path)
            })
            item.appendChild(open)
            item.appendChild(drop)
            list.appendChild(item)
        }
        // Offered only for a document that is open and not already here.
        const keep = document.getElementById('drawer-keep')
        if (keep) {
            const kept = this.places.some((p) => p.path === here)
            keep.hidden = !here || kept
        }
    }

    /// The open document is now another one.
    documentChanged() {
        this.paint()
    }

    async keep() {
        const here = this.current()
        if (!here) return
        try {
            this.places = await invoke('menu_keep', { place: here, name: this.nameForCurrent() })
        } catch (e) {
            console.warn('[Overseer] the document could not be kept', e)
        }
        this.paint()
    }

    async drop(path) {
        try {
            this.places = await invoke('menu_drop', { place: path })
        } catch (e) {
            console.warn('[Overseer] the document could not be taken off the list', e)
        }
        this.paint()
    }

    async open(path) {
        if (!path || path === this.current()) {
            if (this.overlaid()) this.fold(true)
            return
        }
        if (this.inBrowser) {
            this.navigate('/doc/' + String(path).split('/').map(encodeURIComponent).join('/'))
            return
        }
        if (this.overlaid()) this.fold(true)
        await this.app.loadFile(path)
    }

    /// The list is a document: renamed and reordered by opening it.
    async editList() {
        try {
            const where = await invoke('menu_location')
            if (where) await this.open(String(where))
        } catch (e) {
            console.warn('[Overseer] the list of documents could not be opened', e)
        }
    }

    /// Whether the drawer lies over the page rather than beside it.
    overlaid() {
        return typeof window !== 'undefined' && window.innerWidth < 768
    }

    /// Say something that would otherwise go unseen, because the status line is folded away.
    notice(message) {
        if (!this.isFolded() || !message) return
        let toast = document.getElementById('drawer-notice')
        if (!toast) {
            toast = document.createElement('div')
            toast.id = 'drawer-notice'
            toast.className = 'drawer-notice'
            toast.setAttribute('role', 'status')
            document.body.appendChild(toast)
        }
        toast.textContent = message
        toast.classList.add('shown')
        clearTimeout(this._noticeTimer)
        this._noticeTimer = setTimeout(() => toast.classList.remove('shown'), 5000)
    }
}
