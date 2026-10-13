import { describe, it, expect, beforeEach, vi } from 'vitest'

// Every field edit emits a 'change' event. Answering one meant uploading the whole document,
// which measured ~5.5 s for 10.5 MB - and the backend's answer, when the node declares no
// handler, is the document unchanged. A document that handles no change events (exercise.os
// declares none) was paying that on every single edit for nothing.
//
// The frontend now applies the backend's own rule before making the trip: a handler is a child
// `on <event>` block, with one implicit case for mounts on 'load'/'unload'.
//
// When a handler does exist, the trip carries the document's text rather than the document,
// for the same reason the edit path does - Rust owns the document and rebuilds it from text
// that is two orders of magnitude smaller. With no text in hand, the file has it.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
      </div>
      <button id="welcome-open-btn"></button>
      <button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div>
      <div id="content-display"></div>
      <div id="status-bar">
        <span id="status-message"></span>
        <span id="status-info"></span>
        <span id="file-path"></span>
      </div>
      <div id="welcome-screen" class="screen"></div>
      <div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div>
      <div id="error-message"></div>
    </div>
  `
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

import { OverseerApp } from '../src/main.js'

const button = (name, handlers) => ({
  name, node_type: 'button', parameters: { value: { String: name } },
  is_hierarchy_transparent: false, children: handlers,
  source_id: `s1n_${name}`, source_fingerprint: 1, param_order: [], authored_dash: false,
})

const onBlock = (event) => ({
  name: event, node_type: 'on', parameters: {}, children: [],
  is_hierarchy_transparent: false, source_id: `s1n_on_${event}`, param_order: [],
})

function buildDoc() {
  return [{
    name: 'doc', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false,
    source_id: 's1n1', source_fingerprint: 1, param_order: [], authored_dash: false,
    children: [button('plain', []), button('acts', [onBlock('click')])],
  }]
}

describe('emitting an event', () => {
  let calls
  let sent
  let installDoc
  beforeEach(async () => {
    setupDOM()
    calls = []
    sent = []
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(async (cmd, args) => {
      calls.push(cmd)
      sent.push([cmd, args])
      if (cmd === 'load_overseer_file') return 'FILE-TEXT'
      if (cmd === 'execute_overseer_event_with_text') {
        return { nodes: JSON.parse(JSON.stringify(installDoc)), text: 'DOC-TEXT-AFTER' }
      }
      return null
    })
    installDoc = buildDoc()
  })

  const eventCalls = () => calls.filter(c => c === 'execute_overseer_event_with_text').length

  it('makes no backend call when the node declares no handler', async () => {
    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    const node = app.currentDocument[0].children[0]
    node.__overseer_path = ['doc', 'plain']

    await app.renderer.emitEvent(node, null, 'change')

    expect(calls.length, 'the backend was asked to run a handler that does not exist').toBe(0)
  })

  it('still calls the backend when a handler is declared', async () => {
    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app._currentText = 'DOC-TEXT'
    const node = app.currentDocument[0].children[1]
    node.__overseer_path = ['doc', 'acts']

    await app.renderer.emitEvent(node, null, 'click')

    expect(eventCalls(), 'a declared handler must still run').toBe(1)
  })

  it('still calls the backend for a mount, which acts on load without declaring a handler', async () => {
    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app._currentText = 'DOC-TEXT'
    const mount = {
      name: 'm', node_type: 'mount', parameters: {}, children: [],
      is_hierarchy_transparent: false, source_id: 's1n_m', param_order: [],
    }
    app.currentDocument[0].children.push(mount)
    mount.__overseer_path = ['doc', 'm']

    await app.renderer.emitEvent(mount, null, 'load')

    expect(eventCalls(), 'a mount acts on load with no on-block, so the trip is required').toBe(1)
  })

  it('sends the text rather than the document when a handler runs', async () => {
    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app._currentText = 'DOC-TEXT'
    const node = app.currentDocument[0].children[1]
    node.__overseer_path = ['doc', 'acts']

    await app.renderer.emitEvent(node, null, 'click')

    expect(eventCalls()).toBe(1)
    expect(sent.find(([c]) => c === 'execute_overseer_event_with_text')[1].content).toBe('DOC-TEXT')
    expect(app._currentText, 'the text returned by the event was not kept').toBe('DOC-TEXT-AFTER')
  })

  it('reads the file when no text is known, and never sends the document', async () => {
    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.currentFile = 'doc.os'
    app._currentText = null
    const node = app.currentDocument[0].children[1]
    node.__overseer_path = ['doc', 'acts']

    // The instruction goes unanswered, so the press goes the long way, from the text.
    await app.renderer.emitEvent(node, null, 'click')

    expect(calls.filter(c => c === 'load_overseer_file').length).toBe(1)
    expect(sent.find(([c]) => c === 'execute_overseer_event_with_text')[1].content).toBe('FILE-TEXT')
    expect(sent.some(([, args]) => args && 'nodes' in args), 'the document was sent').toBe(false)
  })
})
