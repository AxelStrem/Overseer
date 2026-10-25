import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A page catches up when a file its document mounts moves.
//
// The server says what the files a document mounts read just before each version of the
// document's own file. A document that mounts a catalogue shows the catalogue too, so a food the
// bot added there changes what the page should show though the document's own file did not move.

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
import { versionOf } from '../src/text_version.js'

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

// What the server answers a catch-up after the mounted file moved: what reads it, worked out again.
const theMountMoved = (readFrom) => ({
  version: versionOf(TEXT),
  changes: [{ kind: 'parameters', path: [0, 0], parameters: { value: { Integer: 9 }, label: { String: 'N' } } }],
  nodes: null,
  wrote: false,
  read_from: readFrom,
})

const catchUps = () => invoke.mock.calls.filter(([cmd]) => cmd === 'catch_up_overseer_document')

const answering = (answer) => invoke.mockImplementation(async (cmd) =>
  cmd === 'catch_up_overseer_document' ? answer : null)

const shownN = () => document.querySelector(`[data-path='${JSON.stringify(['day', 'n'])}'] .field-value`)?.textContent

const settle = async (ms = 0) => { await vi.advanceTimersByTimeAsync(ms) }

// What the server says as the page starts listening, and again each time something moves.
const heard = (readFrom) => {
  app.heardItReads(readFrom)
  app.heardTheFile(versionOf(TEXT))
}

describe('a page catches up when a file its document mounts moves', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
  })
  afterEach(() => { vi.useRealTimers() })

  it('takes what it first hears as what it was shown', async () => {
    anApp()
    heard('reads-1')
    await settle(1000)
    expect(catchUps()).toHaveLength(0)
  })

  it('catches up when only a mounted file moved, saying what it was shown', async () => {
    anApp()
    heard('reads-1')
    answering(theMountMoved('reads-2'))
    heard('reads-2')
    await settle()
    expect(catchUps()).toHaveLength(1)
    expect(catchUps()[0][1].shown_read_from).toBe('reads-1')
    expect(catchUps()[0][1].shown_version).toBe(versionOf(TEXT))
    expect(shownN(), 'what the mounted file changed never reached the page').toBe('9')
    // Heard again on reconnecting: nothing to do.
    heard('reads-2')
    await settle(1000)
    expect(catchUps()).toHaveLength(1)
  })

  it('takes what an edit\'s answer says its mounts read', async () => {
    anApp()
    heard('reads-1')
    app.tookTheText({ version: 'mine-1', read_from: 'reads-2' })
    expect(app.showing().shown_read_from).toBe('reads-2')
    app.alreadyWritten({ wrote: true, version: 'mine-1' })
    app.heardItReads('reads-2')
    app.heardTheFile('mine-1')
    await settle(1000)
    expect(catchUps(), 'caught up with what the edit\'s answer already brought').toHaveLength(0)
  })

  it('does not ask again and again when the answer names the mounts some other way', async () => {
    anApp()
    heard('reads-1')
    answering(theMountMoved('named-otherwise'))
    heard('reads-2')
    await settle(5000)
    expect(catchUps()).toHaveLength(1)
  })
})
