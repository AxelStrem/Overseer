import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from './main.js'
import { PANE_CLASS, paneOpened, paneClosed } from './renderer.js'

/**
 * A second document drawn on the page, beside the one open: the drawer's.
 *
 * It is an app of its own - its document, its text, its presses, edits, folds and clock, its watch
 * on its file - drawn into its own element by a renderer of its own, which finds only what it drew
 * there. Every command it sends names its document, and the one open beside it, which the server
 * tells it of as `host/open`.
 *
 * What belongs to the page stays the page's: the screens, the keys, the status line, the title,
 * and undo. A pane keeps no undo of its own - Ctrl+Z, and a `call (name="undo")` pressed in it,
 * take back the open document's last change - and its calls run against the page (see
 * `runCalls`), so `keep` keeps the open document.
 */
export class DocumentPane extends OverseerApp {
    constructor(host, element) {
        element.classList.add(PANE_CLASS)
        element.replaceChildren()
        const tabs = document.createElement('div')
        tabs.className = 'overseer-pane-tabs'
        const content = document.createElement('div')
        content.className = 'overseer-pane-content'
        element.append(tabs, content)
        super({ root: element, content, tabs })
        this.host = host
        this.element = element
        this._stopWatching = null
        paneOpened()
    }

    /// The command, about this pane's document, with the one open beside it. The names are
    /// encoded: the desktop's IPC sends them as real headers, which hold Latin-1 alone, and a
    /// path under a Cyrillic folder would fail every command before it was sent.
    ask(cmd, args) {
        const headers = { 'Overseer-Document': encodeURIComponent(this._opening || this.currentFile || '') }
        const beside = this.host && this.host.currentFile
        if (beside) headers['Overseer-Open'] = encodeURIComponent(String(beside))
        return invoke(cmd, args || {}, { headers })
    }

    /// Open a document in the pane, and watch its file.
    async open(path) {
        this.stopWatching()
        this._opening = path
        const besideAsked = this.host && this.host.currentFile
        try {
            await this.loadFile(path)
        } finally {
            this._opening = null
        }
        if (this.currentFile !== path) return
        this._beside = this.host && this.host.currentFile
        // Opened with the page, the pane may have asked before the document beside it was open,
        // and been told of nothing: told again now, rather than at the next change.
        if (this._beside !== besideAsked) this.besideChanged()
        const watch = typeof window !== 'undefined' ? window.__overseerWatch : null
        if (typeof watch === 'function') this._stopWatching = watch(path, this)
    }

    stopWatching() {
        if (this._stopWatching) this._stopWatching()
        this._stopWatching = null
    }

    /// The pane goes, and nothing of it is left running.
    close() {
        this.stopWatching()
        this.stopTheClock()
        clearTimeout(this._besideTimer)
        if (this.element.classList.contains(PANE_CLASS)) paneClosed()
        this.element.classList.remove(PANE_CLASS)
        this.element.replaceChildren()
        this.currentFile = null
        this.currentDocument = null
    }

    /// The document open beside it was opened or written: what `host/open` says may have moved -
    /// another document, or one that can now be undone - so it is worked out again, as a catch-up
    /// is, keeping what is typed and folded here. Once, for a burst of writes.
    besideChanged() {
        if (!this.currentFile) return
        clearTimeout(this._besideTimer)
        this._besideTimer = setTimeout(() => {
            this._besideTimer = null
            if (!this.currentFile) return
            if (this.busyForCatchingUp()) return this.besideChanged()
            this._beside = this.host && this.host.currentFile
            Promise.resolve(this.catchUp()).catch((e) => {
                console.warn('[Overseer] the drawer could not follow the open document', e)
            })
        }, 50)
    }

    undoLastChange() {
        return this.host && this.host.undoLastChange()
    }

    // The page's own screens, title and status line are not a pane's to change.
    showWelcomeScreen() {}
    showEditorScreen() {}
    showLoadingScreen() {}
    showScreen() {}
    showDocumentName() {}
    updateTitle() {}

    /// A pane says only what went wrong, on the page's status line.
    setStatus(message, info = '', type = 'info') {
        if (type !== 'error' || !this.host) return
        this.host.setStatus(message, info, type)
    }

    /// What went wrong, in the pane, where its document would have been.
    showError(title, error) {
        console.error(title, error)
        const msg = error && error.message ? error.message : (typeof error === 'string' ? error : String(error))
        const said = document.createElement('div')
        said.className = 'overseer-pane-error'
        said.textContent = `${title}: ${msg}`
        this.renderer.contentDisplay.replaceChildren(said)
        this.setStatus(title, msg, 'error')
    }
}
