// Lets the Overseer frontend run in a browser, unchanged.
//
// The frontend talks to the desktop app through exactly one call - invoke(cmd, args) - and
// @tauri-apps/api implements that by calling window.__TAURI_INTERNALS__.invoke. Defining that
// here is enough for the whole application to work against an HTTP server instead: the
// renderer, the DSL and the layout are none the wiser, and there is no second copy of the
// frontend to keep in step with the first.
(function () {
    // Said for the page, which opens a document by going to its address here and by loading its
    // file in the desktop app - see the drawer.
    window.__OVERSEER_HOST__ = 'browser'

    // Which document this tab is looking at. The server has several and, unlike the desktop
    // app, cannot assume - a mount is written relative to the document that declares it.
    //
    // Named either by following a link to the document itself, which is what someone pasting
    // an address will do, or by ?doc= for a page opened at the root.
    const documentName = (() => {
        const path = decodeURIComponent(window.location.pathname || '')
        if (path.startsWith('/doc/')) return path.slice('/doc/'.length)
        return new URLSearchParams(window.location.search).get('doc')
    })()

    // Who is asking, for as long as this tab is open.
    //
    // The server keeps what each viewer is looking at - which day, what is folded - apart from
    // the documents, so two tabs can look at different days without writing over each other.
    // It cannot do that without being told them apart. Kept in sessionStorage so a reload keeps
    // the day it was on, and so a second tab is genuinely a second viewer.
    const session = (() => {
        const minted = () => (crypto.randomUUID ? crypto.randomUUID()
            : `${Date.now()}-${Math.random().toString(36).slice(2)}`)
        try {
            const held = sessionStorage.getItem('overseer.session')
            if (held) return held
            const fresh = minted()
            sessionStorage.setItem('overseer.session', fresh)
            return fresh
        } catch (_) {
            // Private windows and blocked storage: a session that lasts as long as the page is
            // still better than everyone sharing one.
            return minted()
        }
    })()

    async function call(cmd, args) {
        const response = await fetch(`/api/${encodeURIComponent(cmd)}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ document: documentName, session, args: args || {} }),
        })
        const body = await response.json().catch(() => null)
        if (!response.ok) {
            throw new Error((body && body.error) || `${cmd} failed (${response.status})`)
        }
        return body === null ? null : body.result
    }

    window.__TAURI_INTERNALS__ = {
        invoke: (cmd, args) => {
            // Raw byte payloads exist to avoid the desktop IPC's cost for a large document.
            // That cost is not this transport's, and a body of bytes would only have to be
            // decoded again, so it travels as JSON here.
            if (args instanceof Uint8Array || args instanceof ArrayBuffer) {
                const text = new TextDecoder().decode(args)
                return call(cmd, JSON.parse(text))
            }
            return call(cmd, args)
        },
        // Present so nothing that probes for it has to special-case its absence.
        transformCallback: (callback) => callback,
        convertFileSrc: (path) => path,
    }

    if (!documentName) {
        window.addEventListener('DOMContentLoaded', () => {
            const target = document.getElementById('status-message')
            if (target) {
                target.textContent = 'Choose a document from the menu, or open one by its address, for example /doc/tasks.os'
            }
        })
        return
    }

    // The desktop app opens a document through a file dialog, which a browser has no business
    // showing for files on the server. The name in the URL takes its place.
    window.addEventListener('DOMContentLoaded', () => {
        const open = () => {
            if (!window.app || typeof window.app.loadFile !== 'function') {
                return setTimeout(open, 20)
            }
            window.app.loadFile(documentName).catch((e) => {
                console.error('could not open', documentName, e)
            })
        }
        open()
    })
})()
