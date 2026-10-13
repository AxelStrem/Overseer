import { describe, it, expect, beforeEach, vi } from 'vitest'

// What is typed into a field's editor and not yet written survives a reload of the browser tab.
//
// The page keeps it per tab, as it keeps folds and the chosen tab, under the field's place and
// the key of the entry it is in; the first drawing after a reload opens the field again with the
// text and the caret as they were. Finishing the edit, or giving it up, lets it go. A field the
// place no longer holds gets nothing, and leaving the window does not finish the edit.

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

const base = (name, node_type, parameters = {}, children = [], transparent = false) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: transparent,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const task = (n, key) => base(`Task__${n}`, 'div', {}, [
  base('key', 'string', { value: { String: key }, hidden: { Boolean: true } }),
  base('title', 'string', { value: { String: `title ${key}` } }),
  base('notes', 'text', { value: { String: `notes ${key}` } }),
])

const theDocument = (keys = ['a', 'b']) => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Open', 'list', { key: { String: 'key' } }, keys.map((key, i) => task(i + 1, key))),
])]

let app

// A page drawn afresh, as after a reload: a new app over the same browser tab's storage.
const open = (doc = theDocument()) => {
  app = new OverseerApp()
  app.currentDocument = doc
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const reload = (doc = theDocument()) => {
  setupDOM()
  open(doc)
  return app.renderer.putBackDraft()
}

const drawn = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', 'Open', ...path])}']`)
const valueOf = (...path) => drawn(...path).querySelector('.field-value')
const editor = () => document.querySelector('input.field-editor, textarea.field-editor, .markdown-editor-container textarea')
const kept = () => JSON.parse(sessionStorage.getItem('overseer.draft:t.os') || 'null')
const status = () => document.getElementById('status-message').textContent

const type = (text, start = text.length, end = start) => {
  const box = editor()
  box.value = text
  box.setSelectionRange(start, end)
  box.dispatchEvent(new Event('input', { bubbles: true }))
}

const dblclick = (el) => el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))

beforeEach(() => {
  sessionStorage.clear()
  setupDOM()
  invoke.mockReset()
  invoke.mockImplementation(async () => null)
  open()
})

describe('an unsaved edit survives a reload', () => {
  it('is kept as it is typed, and only once it differs from what the field said', () => {
    dblclick(valueOf('Task__2', 'title'))
    expect(kept()).toBeNull()
    type('half a new ti', 4, 6)
    expect(kept()).toMatchObject({ text: 'half a new ti', was: 'title b', start: 4, end: 6 })
    type('title b')
    expect(kept()).toBeNull()
  })

  it('opens the field again after a reload, with the text and the caret as they were', () => {
    dblclick(valueOf('Task__2', 'title'))
    type('half a new ti', 4, 6)

    expect(reload()).toBe(true)
    const box = editor()
    expect(box).toBeTruthy()
    expect(box.closest('[data-path]').dataset.path).toBe(JSON.stringify(['t', 'Open', 'Task__2', 'title']))
    expect(box.value).toBe('half a new ti')
    expect([box.selectionStart, box.selectionEnd]).toEqual([4, 6])
    expect(document.activeElement).toBe(box)
    // Nothing reached the file on the way.
    expect(invoke).not.toHaveBeenCalled()
    // And it is still kept, should the page go away again.
    expect(kept()).toMatchObject({ text: 'half a new ti' })
  })

  it('opens a markdown field again in its own editor', () => {
    dblclick(valueOf('Task__1', 'notes'))
    type('notes a, and more')

    expect(reload()).toBe(true)
    const box = editor()
    expect(box.closest('.markdown-editor-container')).toBeTruthy()
    expect(box.value).toBe('notes a, and more')
  })

  it('is let go when the edit is finished or given up', async () => {
    dblclick(valueOf('Task__1', 'title'))
    type('a new title')
    editor().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(kept()).toBeNull()

    dblclick(valueOf('Task__1', 'title'))
    type('a new title')
    editor().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))
    expect(kept()).toBeNull()
    expect(reload()).toBe(false)
    expect(editor()).toBeNull()
  })

  it('goes to the same entry, and nowhere else when its place holds another', () => {
    dblclick(valueOf('Task__2', 'title'))
    type('for b')

    // The file changed meanwhile: the entry that was second is gone, and another is in its place.
    expect(reload(theDocument(['a', 'c']))).toBe(false)
    expect(editor()).toBeNull()
    expect(kept()).toBeNull()
    expect(status()).toMatch(/could not be put back/)
  })

  it('says so when the field was written to elsewhere since the edit began', () => {
    dblclick(valueOf('Task__1', 'title'))
    type('mine')

    const doc = theDocument()
    doc[0].children[0].children[0].children[1].parameters.value = { String: 'theirs' }
    expect(reload(doc)).toBe(true)
    expect(editor().value).toBe('mine')
    expect(status()).toMatch(/changed since/)
  })

  it('is not finished by leaving the window, but by leaving the field', () => {
    dblclick(valueOf('Task__1', 'title'))
    type('still typing')
    const box = editor()

    // The window loses focus: the field keeps it, and the blur says nothing about the field.
    const away = vi.spyOn(document, 'hasFocus').mockReturnValue(false)
    box.dispatchEvent(new FocusEvent('blur'))
    away.mockRestore()
    expect(editor()).toBe(box)
    expect(kept()).toMatchObject({ text: 'still typing' })

    // A press elsewhere on the page moves the focus, and finishes it.
    box.blur()
    expect(editor()).toBeNull()
    expect(kept()).toBeNull()
  })

  it('is put back by opening the document', async () => {
    dblclick(valueOf('Task__1', 'title'))
    type('typed before the reload')

    setupDOM()
    app = new OverseerApp()
    invoke.mockImplementation(async (cmd) => {
      if (cmd === 'load_overseer_file') return 'TEXT'
      if (cmd === 'parse_overseer_content') return theDocument()
      return null
    })
    await app.loadFile('t.os')
    expect(editor()?.value).toBe('typed before the reload')
  })
})
