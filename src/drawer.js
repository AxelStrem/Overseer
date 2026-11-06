import { invoke } from '@tauri-apps/api/core'
import { localeWeekStart } from './renderer.js'

/**
 * The drawer: the strip at the left of the page that holds whoever is looking's own drawer
 * document, and the status line under it.
 *
 * What it shows - the documents kept to hand, what there is to do with the open one, the look, the
 * account - is a document (see the backend's `drawer` and `library/drawer/drawer.os`), drawn in a
 * pane of its own (`pane.js`), so a person can change it as they change any other. What stays the
 * page's is the shell around it: folding it away and back, Ctrl+\, lying over the page on a narrow
 * screen, telling the host whether the computer is light or dark, and saying an error while the
 * status line is folded away.
 */
export class Drawer {
    constructor(app) {
        this.app = app
        // The bridge says so when the page is running in a browser against the server; the
        // desktop app has no bridge.
        this.inBrowser = typeof window !== 'undefined' && window.__OVERSEER_HOST__ === 'browser'
        this.element = document.getElementById('drawer')
    }

    /// Wire it up. Nothing to do on a page without one.
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
        this.watchTheScheme()
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
        return this.refreshSettings()
    }

    /// Whether the computer is set light or dark, which a mode of "system" follows.
    scheme() {
        try {
            return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
        } catch (_) {
            return 'light'
        }
    }

    /// Tell the host the scheme, and again whenever the computer switches, drawing the open
    /// document again in it. In a browser the bridge sends it with every request; the desktop app
    /// keeps what it was last told.
    watchTheScheme() {
        const report = () => invoke('report_color_scheme', { scheme: this.scheme(), weekStart: localeWeekStart() }).catch(() => { /* drawn light */ })
        this.reported = report()
        try {
            const query = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)')
            if (!query || !query.addEventListener) return
            query.addEventListener('change', async () => {
                await report()
                if (this.settings && this.settings.mode === 'system') this.redraw()
            })
        } catch (_) { /* follows the scheme it started in */ }
    }

    /// The look chosen, which says whether the page follows the computer's scheme.
    async refreshSettings() {
        try {
            this.settings = await invoke('settings_read')
        } catch (e) {
            console.warn('[Overseer] the settings could not be read', e)
            this.settings = null
        }
    }

    /// The open document, worked out again in the look now chosen - and the drawer with it, which
    /// follows the open document.
    async redraw() {
        if (!this.app || !this.app.currentFile || typeof this.app.loadFile !== 'function') {
            const pane = this.app && this.app.drawerPane
            if (pane && pane.currentFile) await pane.loadFile(pane.currentFile).catch(() => { /* drawn as it was */ })
            return
        }
        try { await this.app.loadFile(this.app.currentFile) } catch (_) { /* drawn as it was */ }
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
        return this.overlaid()
    }

    /// Whether the drawer lies over the page rather than beside it.
    overlaid() {
        return typeof window !== 'undefined' && window.innerWidth < 768
    }

    /// What the open document is called: the label of the tab it is wrapped in - its first node,
    /// when that is a tab, written as one or an instance of a template that is one. Wrapping a
    /// document in a tab is a habit rather than a rule, so there may be none, which leaves it to
    /// the file's name.
    nameOfCurrent() {
        try {
            const doc = this.app.currentDocument
            const first = Array.isArray(doc) ? doc[0] : null
            if (!first || !this.app.renderer.isTab(first)) return ''
            const label = this.app.renderer.getParameterValue(first, 'label')
            return label ? String(label).trim() : ''
        } catch (_) {
            return ''
        }
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
