import { describe, it, expect, beforeEach, vi } from 'vitest'

// A field whose path says `header` was once kept to the page when edited: shown, never sent to the
// backend. The save after it wrote the text the backend had last produced, which had never seen
// the edit, so the new heading was gone on the next load. Every edit goes to the backend, a header
// among them, and what is saved is the text it answers with.

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

const header = {
  name: 'text_header', node_type: 'string',
  parameters: { mutable: { Boolean: true }, value: { String: 'Tasks' } },
  is_hierarchy_transparent: false, children: [],
  source_id: 's1n_header', source_fingerprint: 1, param_order: [], authored_dash: false,
}

const theDocument = () => ([{
  name: 'doc', node_type: 'tab',
  parameters: { mutable: { Boolean: true } },
  is_hierarchy_transparent: false,
  source_id: 's1n1', source_fingerprint: 1, param_order: [], authored_dash: false,
  children: [header],
}])

const editHeader = (text) => {
  const el = document.querySelector(`[data-path='${JSON.stringify(['doc', 'text_header'])}']`)
  const holder = el.querySelector('.field-value, .text-content') || el
  holder.dispatchEvent(new Event('dblclick', { bubbles: true }))
  const input = document.querySelector('input.field-editor, textarea.field-editor')
  input.value = text
  input.dispatchEvent(new Event('blur'))
}

const settle = () => new Promise(r => setTimeout(r, 50))
const calls = (name) => invoke.mock.calls.filter(([cmd]) => cmd === name)

describe('a header edit', () => {
  let app

  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    app = new OverseerApp()
    app.currentDocument = theDocument()
    app.currentFile = 'doc.os'
    app._currentText = 'TEXT-AS-OPENED'
    app._originalText = 'TEXT-AS-OPENED'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('is sent to the backend like any other string', async () => {
    invoke.mockImplementation(async () => null)
    editHeader('Chores')
    await settle()
    expect(calls('write_overseer_values')).toEqual([['write_overseer_values', {
      path: 'doc.os',
      values: [{ node_path: ['doc', 'text_header'], value: { String: 'Chores' } }],
    }]])
  })

  it('is in the text that is saved', async () => {
    invoke.mockImplementation(async (cmd) => cmd === 'write_overseer_values'
      ? { wrote: true, changes: [], nodes: null, text: 'TEXT-WITH-CHORES' }
      : null)
    editHeader('Chores')
    await settle()
    await app.saveFile()
    const saved = calls('save_overseer_file_from_text')
    expect(saved.length).toBe(1)
    expect(saved[0][1].content, 'the save sent text that had not seen the edit').toBe('TEXT-WITH-CHORES')
  })
})
