import { describe, it, expect, beforeEach, vi } from 'vitest'

// What is typed into a text field's editor survives the div it is in being folded away: unfolded
// again, the field is open as it was, with the text and the caret. The same holds through a
// reload while the div is folded. A field left on the page is written as it is left, so pressing
// the fold header finishes a one-line field; the text editor is closed only by its own buttons.

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
  base('Notes', 'div', { foldable: { Boolean: true }, label: { String: 'Notes' } }, [
    base('title', 'string', { value: { String: `title ${key}` } }),
    base('notes', 'text', { value: { String: `notes ${key}` } }),
  ]),
])

const theDocument = () => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Open', 'list', { key: { String: 'key' } }, [task(1, 'a'), task(2, 'b')]),
])]

let app

const open = (doc = theDocument()) => {
  app = new OverseerApp()
  app.currentDocument = doc
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const reload = () => {
  setupDOM()
  open()
  return app.renderer.putBackDraft()
}

const drawn = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', 'Open', ...path])}']`)
const valueOf = (...path) => drawn(...path).querySelector('.field-value')
const editor = () => document.querySelector('input.field-editor, textarea.field-editor, .markdown-editor-container textarea')
const kept = () => JSON.parse(sessionStorage.getItem('overseer.draft:t.os') || 'null')
const header = (entry) => drawn(entry, 'Notes')?.querySelector(':scope > .overseer-fold-header')
const press = (entry) => header(entry).dispatchEvent(new MouseEvent('click', { bubbles: true }))

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

describe('an unsaved edit survives its div being folded', () => {
  it('opens the text editor again when the div is unfolded, as it was', () => {
    dblclick(valueOf('Task__2', 'Notes', 'notes'))
    type('notes b, and half a thought', 7, 9)

    press('Task__2')
    expect(drawn('Task__2', 'Notes', 'notes')).toBeNull()
    expect(editor()).toBeNull()
    expect(kept()).toMatchObject({ text: 'notes b, and half a thought' })

    press('Task__2')
    const box = editor()
    expect(box?.closest('.markdown-editor-container')).toBeTruthy()
    expect(box.closest('[data-path]').dataset.path)
      .toBe(JSON.stringify(['t', 'Open', 'Task__2', 'Notes', 'notes']))
    expect(box.value).toBe('notes b, and half a thought')
    expect([box.selectionStart, box.selectionEnd]).toEqual([7, 9])
    expect(invoke).not.toHaveBeenCalled()
    expect(kept()).toMatchObject({ text: 'notes b, and half a thought' })
  })

  it('keeps it through a reload while the div is folded, until it is unfolded', () => {
    dblclick(valueOf('Task__1', 'Notes', 'notes'))
    type('notes a, typed on')
    press('Task__1')

    expect(reload()).toBe(false)
    expect(editor()).toBeNull()
    expect(kept()).toMatchObject({ text: 'notes a, typed on' })
    expect(document.getElementById('status-message').textContent).not.toMatch(/could not be put back/)

    press('Task__1')
    expect(editor()?.value).toBe('notes a, typed on')
  })

  it('opens nothing again once the edit was saved or given up', () => {
    dblclick(valueOf('Task__1', 'Notes', 'notes'))
    type('notes a, saved')
    document.querySelector('.markdown-editor-container .save-btn').click()
    press('Task__1')
    press('Task__1')
    expect(editor()).toBeNull()

    dblclick(valueOf('Task__1', 'Notes', 'notes'))
    type('notes a, given up')
    document.querySelector('.markdown-editor-container .cancel-btn').click()
    press('Task__1')
    press('Task__1')
    expect(editor()).toBeNull()
    expect(kept()).toBeNull()
  })

  it('still finishes a one-line field that is left for the fold header', () => {
    dblclick(valueOf('Task__1', 'Notes', 'title'))
    type('a new title')
    // A press on the header moves the focus there first.
    editor().blur()
    press('Task__1')
    press('Task__1')
    expect(editor()).toBeNull()
    expect(kept()).toBeNull()
  })
})
