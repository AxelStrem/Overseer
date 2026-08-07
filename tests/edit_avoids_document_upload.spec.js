import { describe, it, expect, beforeEach, vi } from 'vitest'

// Editing a field used to upload the whole document so the backend could serialize it. On a
// large document that measured ~5.5 s for 10.5 MB - the IPC moves a couple of MB per second,
// and it ran on every edit. The document's text is a fraction of that size and the backend
// returns it with each resolve, so a run of ordinary field edits should send nothing but the
// text and the changed values.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
        <button id="save-file-btn"></button>
        <button id="reload-file-btn"></button>
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

const deepClone = (o) => JSON.parse(JSON.stringify(o))

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

/** A backend that answers with both the document and its text, as the real one does. */
function installBackend(calls) {
  return async (cmd, args) => {
    calls.push(cmd)
    if (cmd === 'get_next_timer_due_ms' || cmd === 'scheduler_tick') return null
    if (cmd === 'serialize_overseer_nodes' || cmd === 'serialize_overseer_nodes_raw') return 'DOC-TEXT'
    if (cmd === 'parse_overseer_content_selective_with_text') {
      installBackend.lastContent = args.content
      return { nodes: deepClone(installBackend.doc), text: 'DOC-TEXT' }
    }
    if (cmd === 'parse_overseer_content_selective' || cmd === 'parse_overseer_content') {
      return deepClone(installBackend.doc)
    }
    if (cmd === 'execute_overseer_event') return deepClone(args.nodes)
    return null
  }
}

const uploadedDocument = (calls) =>
  calls.filter(c => c === 'serialize_overseer_nodes' || c === 'serialize_overseer_nodes_raw').length

describe('editing a field on a large document', () => {
  beforeEach(() => setupDOM())

  it('does not upload the document once its text is known', async () => {
    const calls = []
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(installBackend(calls))

    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    installBackend.doc = buildDoc()
    // The text is known from the moment the file was read.
    app._currentText = 'DOC-TEXT'
    app.renderer.renderDocument(app.currentDocument)

    editField('alpha', 'edited')
    await new Promise(r => setTimeout(r, 50))

    expect(uploadedDocument(calls), 'the document was uploaded despite its text being known').toBe(0)
    expect(installBackend.lastContent, 'the known text was not sent').toBe('DOC-TEXT')
  })

  it('uploads once to recover the text, then stops', async () => {
    const calls = []
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(installBackend(calls))

    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    installBackend.doc = buildDoc()
    // No text in hand - an event restructured the document, say.
    app._currentText = null
    app.renderer.renderDocument(app.currentDocument)

    editField('alpha', 'first')
    await new Promise(r => setTimeout(r, 50))
    expect(uploadedDocument(calls), 'the document should be uploaded to rebuild the text').toBe(1)

    editField('beta', 'second')
    await new Promise(r => setTimeout(r, 50))
    expect(
      uploadedDocument(calls),
      'the text returned by the first edit should have spared the second an upload'
    ).toBe(1)
  })
})
