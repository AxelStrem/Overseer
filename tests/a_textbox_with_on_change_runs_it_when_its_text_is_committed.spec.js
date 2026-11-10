import { describe, it, expect, beforeEach, vi } from 'vitest'

// A textbox that says `on change` is pressed by its own text: once what is typed is committed,
// Enter or leaving the box, its actions run with the text sent along. The drawer's path box asks
// the host to `describe` the document that way, for the title its name box offers.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        </div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'
import { runCall } from '../src/calls.js'

const base = (name, node_type, parameters = {}, children = [], transparent = false) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: transparent,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const describing = () => base('change', 'on', {}, [base('call', 'call', { name: { String: 'describe' } })])

// t / [ path (on change), plain ]
const theDocument = () => [base('t', 'tab', { mutable: { String: 'guarded' } }, [
  base('path', 'textbox', { value: { String: '' } }, [describing()]),
  base('plain', 'textbox', { value: { String: '' } }),
  base('secret', 'textbox', { value: { String: '' }, kind: { String: 'secret' } }, [describing()]),
])]

let app

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

const boxOf = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}'] input`)

const commit = (name, text) => {
  const input = boxOf(name)
  input.value = text
  input.dispatchEvent(new Event('input'))
  input.dispatchEvent(new Event('change'))
}

const events = () => invoke.mock.calls.filter(([cmd]) => cmd === 'run_overseer_event').map(([, args]) => args)

const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('a textbox with on change', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation(async (cmd) =>
      cmd === 'run_overseer_event' ? { text: 'TEXT', changes: [], nodes: null, wrote: false, calls: [] } : null)
  })

  it('runs it once what is typed is committed, with the text sent along', async () => {
    render()
    commit('path', 'notes.os')
    await settled()
    const sent = events()
    expect(sent).toHaveLength(1)
    expect(JSON.stringify(sent[0])).toContain('change')
    expect(JSON.stringify(sent[0])).toContain('notes.os')
  })

  it('runs nothing for a box that says nothing, nor for a secret one', async () => {
    render()
    commit('plain', 'kept')
    commit('secret', 'hunter2')
    await settled()
    expect(events()).toHaveLength(0)
  })
})

describe('describing a document', () => {
  beforeEach(() => invoke.mockReset())

  it('answers the label the host reads for its path', async () => {
    invoke.mockImplementation(async (cmd, args) =>
      cmd === 'document_label' && args.path === 'notes.os' ? 'My notes' : null)
    expect(await runCall({ name: 'describe', args: { path: 'notes.os' } }, {}))
      .toEqual({ answer: 'My notes', failed: false })
  })

  it('answers nothing for a document with no label, and asks nothing for no path', async () => {
    invoke.mockImplementation(async () => '')
    expect(await runCall({ name: 'describe', args: { path: 'nowhere.os' } }, {}))
      .toEqual({ answer: '', failed: false })
    invoke.mockReset()
    expect(await runCall({ name: 'describe', args: { path: '' } }, {}))
      .toEqual({ answer: '', failed: false })
    expect(invoke).not.toHaveBeenCalled()
  })
})
