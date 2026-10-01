import { describe, it, expect, beforeEach, vi } from 'vitest'

// `start_editing`: a press that opens a field for editing, as though it had been double-tapped.
//
// For the field with nowhere to tap - a comment on a task, hidden while it is empty so that the
// task stays one line. The backend finds the field and the answer names it by child indices; the
// page draws it for as long as it is open, and hides it again if it is closed as empty as it was.

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

// t / Open / Task__1 / [ key, (row: title, note), comment ]
const COMMENT = [0, 0, 0, 2]

// `hiddenForGood`: hidden by its declaration, as a finished task's commentary is in a project.
const theDocument = ({ mutable = true, comment = '', hiddenForGood = false } = {}) => [base('t', 'tab', { mutable: { Boolean: mutable } }, [
  base('Open', 'list', { key: { String: 'key' } }, [
    base('Task__1', 'div', {}, [
      base('key', 'string', { value: { String: 'a' }, hidden: { Boolean: true } }),
      base('div', 'div', { layout: { String: 'horizontal' } }, [
        base('title', 'string', { value: { String: 'Clean the stove' } }),
        base('note', 'button', { icon: { String: 'note' } }, [
          base('click', 'on', {}, [base('start_editing', 'start_editing', { path: { String: '../comment' } })]),
        ]),
      ], true),
      base('comment', 'string', hiddenForGood
        ? { value: { String: comment }, hidden: { Boolean: true } }
        : {
          value: { String: comment },
          hidden: { Formula: 'comment == ""' },
          _computed_hidden: { Boolean: comment === '' },
        }),
    ]),
  ]),
])]

let app

const render = (options) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(options)
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const commentNode = () => {
  let level = app.currentDocument
  let node = null
  for (const i of COMMENT) { node = level[i]; level = node.children }
  return node
}

// The comment as drawn, if it is.
const drawnComment = () => document.querySelector(`[data-path='${JSON.stringify(['t', 'Open', 'Task__1', 'comment'])}']`)
const editor = () => document.querySelector('input.field-editor, textarea.field-editor')

const answering = (startEditing) => invoke.mockImplementation(async (cmd) =>
  cmd === 'run_overseer_event'
    ? { text: 'TEXT', changes: [], nodes: null, wrote: false, start_editing: startEditing }
    : null)

const pressNote = async () => {
  const button = app.currentDocument[0].children[0].children[0].children[1].children[1]
  await app.renderer.emitEvent(
    button,
    document.querySelector(`[data-path='${JSON.stringify(['t', 'Open', 'Task__1', 'note'])}']`),
    'click',
  )
}

describe('a press that opens a field', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    answering({ address: 't/Open/[a]/comment', path: COMMENT })
  })

  it('starts with the empty comment taking no room', () => {
    render()
    expect(drawnComment()).toBeNull()
  })

  it('draws the hidden field and puts the cursor in it', async () => {
    render()
    await pressNote()
    expect(calls('run_overseer_event').length).toBe(1)
    expect(drawnComment(), 'the comment was not drawn').not.toBeNull()
    expect(editor(), 'no editor was opened').not.toBeNull()
    expect(document.activeElement).toBe(editor())
  })

  it('hides it again when it is left empty', async () => {
    render()
    await pressNote()
    editor().dispatchEvent(new window.Event('blur'))
    expect(drawnComment(), 'an empty comment is still taking room').toBeNull()
  })

  it('hides it again when the edit is abandoned', async () => {
    render()
    await pressNote()
    editor().dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape' }))
    expect(drawnComment()).toBeNull()
  })

  it('keeps it showing once something is typed', async () => {
    render()
    await pressNote()
    const box = editor()
    box.value = 'out of degreaser'
    box.dispatchEvent(new window.Event('blur'))
    expect(drawnComment(), 'the typed comment vanished before its write came back').not.toBeNull()
    expect(app.renderer.isOpenForEditing(commentNode())).toBe(false)
  })

  it('opens a field hidden for good, and puts it away once it is closed with something in it', async () => {
    // No write will hide it, since it is hidden whatever it says - so it would sit on screen,
    // looking like a field that shows, until something happened to draw its row again.
    render({ hiddenForGood: true, comment: 'already said' })
    expect(drawnComment()).toBeNull()
    await pressNote()
    expect(editor().value).toBe('already said')
    const box = editor()
    box.value = 'said again'
    box.dispatchEvent(new window.Event('blur'))
    expect(drawnComment(), 'a field hidden for good was left drawn').toBeNull()
  })

  it('opens a comment that is already showing', async () => {
    render({ comment: 'already said' })
    expect(drawnComment()).not.toBeNull()
    await pressNote()
    expect(editor()).not.toBeNull()
    expect(editor().value).toBe('already said')
  })

  it('opens nothing where nothing may change', async () => {
    render({ mutable: false })
    await pressNote()
    expect(editor()).toBeNull()
    expect(drawnComment(), 'a field that could not be opened was left drawn').toBeNull()
  })

  it('does nothing when the answer names no field', async () => {
    answering(undefined)
    render()
    await pressNote()
    expect(editor()).toBeNull()
    expect(drawnComment()).toBeNull()
  })
})

function calls(name) {
  return invoke.mock.calls.filter(([cmd]) => cmd === name)
}
