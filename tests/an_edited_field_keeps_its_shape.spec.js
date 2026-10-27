import { describe, it, expect, beforeEach, vi } from 'vitest'

// An edited field keeps the shape it was drawn with.
//
// A field's value is drawn as a block (a number, so that it lines up under its heading) or an
// inline-block (the rest), set as an inline style. The editor hid the value while it was open and
// showed it again as `inline` - and an inline element's padding and minimum height do not count
// towards the field's height, so the field came out shorter than its neighbours and, in a centred
// row, a few pixels off them until the row was next drawn.

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
import { OverseerApp } from '../src/main.js'

const base = (name, node_type, parameters = {}, children = [], transparent = false) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: transparent,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const theDocument = () => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('div', 'div', { layout: { String: 'horizontal' }, alignment: { String: 'center' } }, [
    base('sets', 'int', { label: { String: 'Sets' }, layout: { String: 'vertical' }, value: { Integer: 3 } }),
    base('reps', 'int', { label: { String: 'Reps' }, layout: { String: 'vertical' }, value: { Integer: 10 } }),
    base('name', 'string', { label: { String: 'Name' }, layout: { String: 'vertical' }, value: { String: 'Curls' } }),
    base('notes', 'text', { label: { String: 'Notes' }, layout: { String: 'vertical' }, value: { String: 'Slowly' } }),
  ], true),
])]

let app

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

const valueOf = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}'] .field-value`)
const editor = () => document.querySelector('input.field-editor')

const openEditorOn = (name) => {
  valueOf(name).dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
  expect(editor()).not.toBeNull()
  expect(valueOf(name).style.display).toBe('none')
}

describe('a field edited in a row', () => {
  beforeEach(() => setupDOM())

  it('is shown again as it was drawn, and as its neighbours are, once the edit is saved', () => {
    render()
    const before = valueOf('reps').style.display
    expect(before).toBe(valueOf('sets').style.display)

    openEditorOn('reps')
    editor().value = '12'
    editor().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))

    expect(editor()).toBeNull()
    expect(valueOf('reps').style.display).toBe(before)
    expect(valueOf('reps').style.display).toBe(valueOf('sets').style.display)
  })

  it('is shown again as it was drawn once the edit is abandoned', () => {
    render()
    const before = valueOf('reps').style.display

    openEditorOn('reps')
    editor().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))

    expect(editor()).toBeNull()
    expect(valueOf('reps').style.display).toBe(before)
  })

  it('keeps the shape of a field that is not a number, too', () => {
    render()
    const before = valueOf('name').style.display

    openEditorOn('name')
    editor().value = 'Rows'
    editor().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))

    expect(valueOf('name').style.display).toBe(before)
  })

  it('keeps the shape of a markdown text closed in its own editor', () => {
    render()
    const before = valueOf('notes').style.display

    valueOf('notes').dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    const textarea = document.querySelector('textarea.markdown-editor')
    expect(textarea).not.toBeNull()
    textarea.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))

    expect(document.querySelector('textarea.markdown-editor')).toBeNull()
    expect(valueOf('notes').style.display).toBe(before)
  })
})
