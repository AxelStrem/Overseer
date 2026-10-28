import { describe, it, expect, beforeEach, vi } from 'vitest'

// A document's `on load` runs when the page opens it - and only then. The server runs it for this
// viewer and answers the way a catch-up is answered; a refusal is said, and the document stays
// open as the file has it.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const aDocument = (withOnLoad) => [base('board', 'tab', { mutable: { Boolean: true } }, [
  base('started', 'string', { value: { String: 'never' }, label: { String: 'Started' } }),
  ...(withOnLoad ? [base('load', 'on', {}, [base('', 'set_now_ts', { path: { String: '/board/started' } })])] : []),
])]

// What the server answers: the guarded field, set for this viewer.
const theLoad = {
  version: 'v1',
  changes: [{ kind: 'parameters', path: [0, 0], parameters: { value: { String: 'just now' }, label: { String: 'Started' } } }],
  nodes: null,
  wrote: false,
}

const loads = () => invoke.mock.calls.filter(([cmd]) => cmd === 'run_overseer_on_load')

const opening = (doc, onLoad) => invoke.mockImplementation(async (cmd) => {
  if (cmd === 'load_overseer_file') return 'tab board {}\n'
  if (cmd === 'parse_overseer_content') return doc
  if (cmd === 'run_overseer_on_load') return onLoad()
  return null
})

const shownStarted = () =>
  document.querySelector(`[data-path='${JSON.stringify(['board', 'started'])}'] .field-value`)?.textContent

describe('a page runs on load when it opens a document', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
  })

  it('asks the server to run it once, and shows what it set', async () => {
    const app = new OverseerApp()
    opening(aDocument(true), () => theLoad)
    await app.loadFile('board.os')
    expect(loads()).toHaveLength(1)
    expect(loads()[0][1].path).toBe('board.os')
    expect(shownStarted()).toBe('just now')
  })

  it('does not ask when the document has no on load', async () => {
    const app = new OverseerApp()
    opening(aDocument(false), () => theLoad)
    await app.loadFile('board.os')
    expect(loads()).toHaveLength(0)
  })

  it('says a refusal, and keeps the document open as the file has it', async () => {
    const app = new OverseerApp()
    opening(aDocument(true), () => { throw new Error('on load may write only guarded fields') })
    await app.loadFile('board.os')
    expect(document.getElementById('status-message').textContent).toBe('on load did not run')
    expect(document.getElementById('status-message').classList.contains('warning')).toBe(true)
    expect(shownStarted()).toBe('never')
  })
})
