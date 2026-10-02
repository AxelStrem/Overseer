import { describe, it, expect, beforeEach, vi } from 'vitest'

// An answer to an instruction names the document's text by version instead of carrying it.
//
// The text was nearly all of every answer - 31 KB of the 36 a tap on tasks.os cost a phone,
// compressed - and the page reads it only when something goes the long way: a write refused and
// worked out from the text instead, or a save. So the page keeps the version, asks for the text
// by it when one of those happens, and otherwise never asks. What has to hold is the last part
// as much as the first: a page that fetched the text, or rebuilt it from the document, before
// every edit just in case would pay more than the text ever cost.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button></div>
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

const field = (name, value) => ({
  name, node_type: 'string',
  parameters: { mutable: { Boolean: true }, value: { String: value } },
  is_hierarchy_transparent: false, children: [],
  source_id: `s1n_${name}`, source_fingerprint: 1, param_order: [], authored_dash: false,
})

const buildDoc = () => ([{
  name: 'doc', node_type: 'tab',
  parameters: { mutable: { Boolean: true } },
  is_hierarchy_transparent: false,
  source_id: 's1n1', source_fingerprint: 1, param_order: [], authored_dash: false,
  children: [field('alpha', 'one'), field('beta', 'two')],
}])

function editField(name, text) {
  const el = Array.from(document.querySelectorAll('[data-path]')).find(e => {
    try { return JSON.parse(e.dataset.path || '[]').slice(-1)[0] === name } catch { return false }
  })
  const holder = el.querySelector('.field-value, .text-content') || el
  holder.dispatchEvent(new Event('dblclick', { bubbles: true }))
  const input = document.querySelector('input.field-editor, textarea.field-editor')
  input.value = text
  input.dispatchEvent(new Event('blur'))
}

const settle = () => new Promise(r => setTimeout(r, 50))
const calls = (name) => invoke.mock.calls.filter(([cmd]) => cmd === name)

// An answer the way the backend now sends one for a described change: no text, a version.
const named = (version, extra = {}) => ({
  version,
  wrote: true,
  changes: [{ kind: 'parameters', address: 'doc/beta', path: [0, 1],
    parameters: { mutable: { Boolean: true }, value: { String: 'recomputed' } } }],
  nodes: null,
  ...extra,
})

describe('an answer that names the text', () => {
  let app, serialized

  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.currentFile = 'doc.os'
    app._currentText = 'TEXT-AS-OPENED'
    app._originalText = 'TEXT-AS-OPENED'
    app.renderer.renderDocument(app.currentDocument)
    serialized = 0
    app.serializeNodes = async () => { serialized++; return 'TEXT-REBUILT' }
  })

  it('keeps the version and asks for nothing more', async () => {
    invoke.mockImplementation(async (cmd) => cmd === 'write_overseer_values' ? named('V1') : null)
    editField('alpha', 'edited')
    await settle()
    expect(app._currentText).toBe(null)
    expect(app._textVersion).toBe('V1')
    expect(calls('load_overseer_text').length, 'the text was fetched for nothing').toBe(0)
    expect(serialized, 'the document was uploaded for nothing').toBe(0)
  })

  it('sends the next change without the text as well', async () => {
    let n = 0
    invoke.mockImplementation(async (cmd) => cmd === 'write_overseer_values' ? named(`V${++n}`) : null)
    editField('alpha', 'edited')
    await settle()
    editField('alpha', 'edited again')
    await settle()
    expect(calls('write_overseer_values').length).toBe(2)
    expect(calls('load_overseer_text').length, 'the text was fetched before an edit').toBe(0)
    expect(serialized, 'the document was uploaded before an edit').toBe(0)
    expect(app._textVersion).toBe('V2')
  })

  it('fetches the text by its version when a write is refused', async () => {
    let refuse = false
    invoke.mockImplementation(async (cmd, args) => {
      if (cmd === 'write_overseer_values') {
        if (refuse) throw new Error('refused')
        return named('V1')
      }
      if (cmd === 'load_overseer_text') return args.version === 'V1' ? 'TEXT-V1' : null
      if (cmd === 'parse_overseer_content_selective_with_text') return { nodes: buildDoc(), text: 'TEXT-AFTER' }
      return null
    })
    editField('alpha', 'edited')
    await settle()
    refuse = true
    editField('alpha', 'edited again')
    await settle()

    expect(calls('load_overseer_text').map(([, a]) => a)).toEqual([{ path: 'doc.os', version: 'V1' }])
    const longWay = calls('parse_overseer_content_selective_with_text')
    expect(longWay.length, 'the refused write did not go the long way').toBe(1)
    expect(longWay[0][1].content, 'the long way did not start from the named text').toBe('TEXT-V1')
    expect(serialized, 'the document was uploaded although its text was there to fetch').toBe(0)
  })

  it('rebuilds the text from the document when the version is no longer kept', async () => {
    let refuse = false
    invoke.mockImplementation(async (cmd) => {
      if (cmd === 'write_overseer_values') {
        if (refuse) throw new Error('refused')
        return named('V1')
      }
      if (cmd === 'load_overseer_text') return null
      if (cmd === 'parse_overseer_content_selective_with_text') return { nodes: buildDoc(), text: 'TEXT-AFTER' }
      return null
    })
    editField('alpha', 'edited')
    await settle()
    refuse = true
    editField('alpha', 'edited again')
    await settle()

    expect(serialized, 'the text was neither fetched nor rebuilt').toBe(1)
    expect(calls('parse_overseer_content_selective_with_text')[0][1].content).toBe('TEXT-REBUILT')
  })

  it('checks a save against the file by version when that is all it was told', async () => {
    app.alreadyWritten({ wrote: true, version: 'V1', file_version: 'F1' })
    expect(app._originalText).toBe(null)
    expect(app._originalVersion).toBe('F1')

    invoke.mockImplementation(async (cmd) => cmd === 'load_overseer_text' ? 'TEXT-V1' : null)
    app._textVersion = 'V1'
    app._currentText = null
    await app.saveFile()

    const saved = calls('save_overseer_file_from_text')
    expect(saved.length).toBe(1)
    expect(saved[0][1].content).toBe('TEXT-V1')
    // Both spellings: the server reads one and the desktop app the other.
    expect(saved[0][1].original_version).toBe('F1')
    expect(saved[0][1].originalVersion).toBe('F1')
    expect(saved[0][1].original).toBe(null)
  })

  it('still takes the text from an answer that carries it', async () => {
    invoke.mockImplementation(async (cmd) =>
      cmd === 'write_overseer_values' ? { ...named('V1'), text: 'TEXT-SENT' } : null)
    editField('alpha', 'edited')
    await settle()
    expect(app._currentText).toBe('TEXT-SENT')
    expect(app._textVersion, 'a version kept beside a text in hand').toBe(null)
  })
})

describe('a press after an answer that named the text', () => {
  let app, serialized

  const withButton = () => ([{
    name: 'doc', node_type: 'tab',
    parameters: { mutable: { Boolean: true } },
    is_hierarchy_transparent: false,
    source_id: 's1n1', source_fingerprint: 1, param_order: [], authored_dash: false,
    children: [
      field('alpha', 'one'),
      { name: 'go', node_type: 'button', parameters: { label: { String: 'go' } },
        is_hierarchy_transparent: false, source_id: 's1n_go', source_fingerprint: 1,
        param_order: [], authored_dash: false,
        children: [{ name: 'click', node_type: 'on', parameters: {}, children: [],
          is_hierarchy_transparent: false, source_id: '', source_fingerprint: 1,
          param_order: [], authored_dash: false }] },
    ],
  }])

  const press = async () => {
    const button = app.currentDocument[0].children[1]
    const el = document.querySelector(`[data-path='${JSON.stringify(['doc', 'go'])}']`)
    await app.renderer.emitEvent(button, el, 'click')
  }

  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    app = new OverseerApp()
    window.app = app
    app.currentDocument = withButton()
    app.currentFile = 'doc.os'
    app._currentText = null
    app._textVersion = 'V1'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
    serialized = 0
    app.serializeNodes = async () => { serialized++; return 'TEXT-REBUILT' }
  })

  it('goes as an instruction without fetching the text', async () => {
    invoke.mockImplementation(async (cmd) => cmd === 'run_overseer_event'
      ? { version: 'V2', wrote: true, changes: [], nodes: null }
      : null)
    await press()
    expect(calls('run_overseer_event').length).toBe(1)
    expect(calls('load_overseer_text').length, 'the text was fetched for a press').toBe(0)
    expect(app._textVersion).toBe('V2')
  })

  it('fetches the text by its version when the press is refused', async () => {
    invoke.mockImplementation(async (cmd, args) => {
      if (cmd === 'run_overseer_event') throw new Error('refused')
      if (cmd === 'load_overseer_text') return args.version === 'V1' ? 'TEXT-V1' : null
      if (cmd === 'execute_overseer_event_with_text') return { nodes: withButton(), text: 'TEXT-AFTER' }
      return null
    })
    await press()
    const longWay = calls('execute_overseer_event_with_text')
    expect(longWay.length, 'the refused press did not go the long way').toBe(1)
    expect(longWay[0][1].content).toBe('TEXT-V1')
  })
})
