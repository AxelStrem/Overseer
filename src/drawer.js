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
        this.named()
    }

    /// What the drawer calls a document: the name the list gives it; or the label of the tab it is
    /// wrapped in, as the open one says it now and as the host read the rest when the list was
    /// asked for; or its file's name.
    nameOf(place) {
        if (place.name) return place.name
        const label = (place.path === this.current() && this.labelOfCurrent()) || place.label
        if (label) return String(label)
        const file = String(place.path).split(/[\\/]/).pop() || String(place.path)
        return file.replace(/\.os$/, '')
    }

    /// What the open document is called - as the drawer calls it when it is kept, and otherwise
    /// the label of the tab it is wrapped in. Nothing when neither says, which leaves it to the
    /// file's name.
    nameOfCurrent() {
        const here = this.current()
        if (!here) return ''
        const kept = this.places.find((p) => p.path === here)
        if (kept) return this.nameOf(kept)
        return this.labelOfCurrent()
    }

    /// The label of the tab the open document is wrapped in: its first node, when that is a tab.
    /// Wrapping a document in a tab is a habit rather than a rule, so there may be none.
    labelOfCurrent() {
        try {
            const doc = this.app.currentDocument
            const first = Array.isArray(doc) ? doc[0] : null
            if (!first || (first.node_type || first.type) !== 'tab') return ''
            const label = this.app.renderer.getParameterValue(first, 'label')
            return label ? String(label).trim() : ''
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
        // Documents with no folder first, then the folders in the order their first documents come
        // in the menu, each holding its documents in the menu's order.
        const folders = new Map()
        for (const place of this.places) {
            const folder = String(place.folder || '').trim()
            if (!folder) {
                list.appendChild(this.placeItem(place, here))
                continue
            }
            if (!folders.has(folder)) folders.set(folder, [])
            folders.get(folder).push(place)
        }
        const folded = this.foldedFolders()
        for (const [folder, held] of folders) {
            const holdsHere = held.some((p) => p.path === here)
            const shut = folded.includes(folder) && !holdsHere
            const item = document.createElement('li')
            item.className = 'drawer-folder'
            item.classList.toggle('folded', shut)
            const head = document.createElement('button')
            head.className = 'drawer-folder-head'
            head.setAttribute('aria-expanded', String(!shut))
            const mark = document.createElement('span')
            mark.className = 'drawer-folder-mark'
            mark.textContent = shut ? '▸' : '▾'
            const title = document.createElement('span')
            title.className = 'drawer-folder-name'
            title.textContent = folder
            const count = document.createElement('span')
            count.className = 'drawer-folder-count'
            count.textContent = String(held.length)
            head.append(mark, title, count)
            // The folder holding the open document stays open, or the mark that says which one is
            // open would be hidden.
            head.addEventListener('click', () => {
                if (holdsHere) return
                this.foldFolder(folder, !shut)
                this.paint()
            })
            const inner = document.createElement('ul')
            inner.className = 'drawer-folder-places'
            inner.hidden = shut
            for (const place of held) inner.appendChild(this.placeItem(place, here))
            item.append(head, inner)
            list.appendChild(item)
        }
        // Offered only for a document that is open and not already here.
        const keep = document.getElementById('drawer-keep')
        if (keep) {
            const kept = this.places.some((p) => p.path === here)
            keep.hidden = !here || kept
        }
    }

    /// One document in the list: its name to open it by, and a button to take it off.
    placeItem(place, here) {
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
        return item
    }

    /// The folders folded away, remembered per viewer like the drawer's own fold.
    foldedFolders() {
        try {
            const said = JSON.parse(localStorage.getItem('overseer.drawer.folders') || '[]')
            return Array.isArray(said) ? said.map(String) : []
        } catch (_) {
            return []
        }
    }

    foldFolder(folder, folded) {
        const rest = this.foldedFolders().filter((f) => f !== folder)
        if (folded) rest.push(folder)
        try { localStorage.setItem('overseer.drawer.folders', JSON.stringify(rest)) } catch (_) { /* not kept */ }
    }

    /// What the open document is called may have changed with the list: the window says it too.
    named() {
        try { this.app && this.app.showDocumentName && this.app.showDocumentName() } catch (_) { /* only a title */ }
    }

    /// The open document is now another one.
    documentChanged() {
        this.paint()
    }

    async keep() {
        const here = this.current()
        if (!here) return
        try {
            // With no name of its own, so that it goes on being called whatever it calls itself.
            this.places = await invoke('menu_keep', { place: here })
        } catch (e) {
            console.warn('[Overseer] the document could not be kept', e)
        }
        this.paint()
        this.named()
    }

    async drop(path) {
        try {
            this.places = await invoke('menu_drop', { place: path })
        } catch (e) {
            console.warn('[Overseer] the document could not be taken off the list', e)
        }
        this.paint()
        this.named()
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
