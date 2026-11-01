import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A hidden page catches up when it is shown.
//
// Each write to the file reaches every open page over the watch stream. A tab left open all day
// would have the server work the document out again for each one, for nobody to see; it waits
// until it is shown, and then catches up once with however many writes it missed.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        <button id="undo-file-btn"></button></div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'

const TEXT = 'tab day (mutable=true) {\n    int n = 1\n}\n'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const theDocument = () => [base('day', 'tab', { mutable: { Boolean: true } }, [
  base('n', 'int', { value: { Integer: 1 }, label: { String: 'N' } }),
])]

let app

const anApp = () => {
  app = new OverseerApp()
  app.currentFile = 'day.os'
  app._currentText = TEXT
  app._originalText = TEXT
  app.currentDocument = theDocument()
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

// What the server answers a catch-up: the file is now `version`, and `n` is 9.
const caughtUpTo = (version) => ({
  version,
  changes: [{ kind: 'parameters', path: [0, 0], parameters: { value: { Integer: 9 }, label: { String: 'N' } } }],
  nodes: null,
  wrote: false,
})

const catchUps = () => invoke.mock.calls.filter(([cmd]) => cmd === 'catch_up_overseer_document')

const answering = (answer) => invoke.mockImplementation(async (cmd) =>
  cmd === 'catch_up_overseer_document' ? answer : null)

const shownN = () => document.querySelector(`[data-path='${JSON.stringify(['day', 'n'])}'] .field-value`)?.textContent

const settle = async (ms = 0) => { await vi.advanceTimersByTimeAsync(ms) }

const seen = (state) => {
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state })
  document.dispatchEvent(new Event('visibilitychange'))
}

describe('a hidden page catches up when it is shown', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
    seen('visible')
  })
  afterEach(() => {
    // Each app listens on the one document; an earlier one must not answer a later test's events.
    if (app) app.currentFile = null
    seen('visible')
    vi.useRealTimers()
  })

  it('asks nothing for the writes it hears while hidden, and once when shown', async () => {
    anApp()
    answering(caughtUpTo('v3'))
    seen('hidden')
    app.heardTheFile('v1')
    app.heardTheFile('v2')
    app.heardTheFile('v3')
    await settle(5000)
    expect(catchUps(), 'a hidden page caught up').toHaveLength(0)

    seen('visible')
    await settle(5000)
    expect(catchUps()).toHaveLength(1)
    expect(shownN(), 'what was written never reached the page').toBe('9')
  })

  it('asks nothing when shown with nothing heard meanwhile', async () => {
    anApp()
    seen('hidden')
    seen('visible')
    await settle(5000)
    expect(catchUps()).toHaveLength(0)
  })

  it('still catches up at once while shown', async () => {
    anApp()
    answering(caughtUpTo('v1'))
    app.heardTheFile('v1')
    await settle()
    expect(catchUps()).toHaveLength(1)
  })

  it('does not ask when hidden while it waited for an edit to finish', async () => {
    anApp()
    answering(caughtUpTo('v1'))
    app._saving = true
    app.heardTheFile('v1')
    await settle()
    expect(catchUps()).toHaveLength(0)
    seen('hidden')
    app._saving = false
    await settle(5000)
    expect(catchUps(), 'the wait for the edit went on asking while hidden').toHaveLength(0)

    seen('visible')
    await settle(5000)
    expect(catchUps()).toHaveLength(1)
  })
})
