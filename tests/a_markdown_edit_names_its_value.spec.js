import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

// Every field commits its edit by naming the path and the value, and the change is then written
// by naming both - applied to whatever the file says at that moment, so a write made elsewhere
// in the meantime survives.
//
// A markdown field named only the path. That says "this changed, work out what follows from it",
// which the page can answer by itself - so nothing was asked of the backend at all, and the edit
// reached the file only through the save that the commit had scheduled. The whole document as
// text, written over the file: the one class of trouble the instruction path exists to remove,
// still reachable by editing a note.

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

const aNote = (text) => [{
  name: 'day', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: [{
    name: 'note',
    node_type: 'text',
    parameters: { value: { String: text }, markdown: { Boolean: true }, mutable: { Boolean: true } },
    children: [],
    is_hierarchy_transparent: false,
  }],
}]

const anAppShowingANote = () => {
  const app = new OverseerApp()
  window.app = app
  app.currentFile = '/documents/day.os'
  app._currentText = 'TEXT'
  app._originalText = 'TEXT'
  app._adoptDocumentPreserveRoot(aNote('as it was'))
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

/** Open the editor on the note, type, and save - as a person does. */
const rewriteTheNote = (to) => {
  const shown = document.querySelector('.text-content, .field-value')
  shown.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
  const textarea = document.querySelector('textarea.markdown-editor')
  expect(textarea, 'the editor did not open').toBeTruthy()
  textarea.value = to
  document.querySelector('.markdown-editor-container .save-btn').click()
}

const callsTo = (name) => invoke.mock.calls.filter(([cmd]) => cmd === name)

/// Let everything the commit set off finish. The fallback path does more work before it sends
/// anything - it clones and serializes the document first - so a couple of microtasks would let
/// it look as though nothing had been sent.
const settle = async () => {
  for (let i = 0; i < 20; i += 1) await new Promise((r) => setTimeout(r, 0))
}

describe('editing a note', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation((cmd) =>
      cmd === 'write_overseer_values'
        ? Promise.resolve({ text: 'TEXT', changes: [], nodes: null })
        : Promise.resolve(null))
  })
  afterEach(() => { delete window.app })

  it('goes up as an instruction naming the new text', async () => {
    anAppShowingANote()
    rewriteTheNote('as it is now')
    await settle()

    const sent = callsTo('write_overseer_values')
    expect(sent, 'the edit did not go as an instruction').toHaveLength(1)
    expect(sent[0][1].values).toHaveLength(1)
    expect(sent[0][1].values[0].node_path).toEqual(['day', 'note'])
    expect(sent[0][1].values[0].value).toEqual({ String: 'as it is now' })
  })

  it('leaves no save behind to carry it', async () => {
    // The write happened as part of applying the edit, so the save the commit scheduled has
    // nothing left to do and is called off. Left to fire, it would send the whole document as
    // text - which is how an edit could overwrite a write made in the meantime - and would be a
    // second step to take back for one change.
    vi.useFakeTimers()
    try {
      const app = anAppShowingANote()
      rewriteTheNote('as it is now')
      await vi.runAllTimersAsync()

      const saved = invoke.mock.calls.filter(([cmd]) => cmd.startsWith('save_overseer_file'))
      expect(saved, 'the document was sent as text as well').toHaveLength(0)
      expect(app.isDocumentModified, 'it still thinks something is unsaved').toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })
})
