import { describe, it, expect, beforeEach, vi } from 'vitest'

// A textbox: a field drawn as a box to type into, which takes the cursor with one tap.
//
// What is typed belongs to the page. Typing sends nothing anywhere - no write, no resolve - and
// the file only ever says what a box starts with. The text reaches the backend once, with a
// press, which is how a form copied into a new entry knows what was typed; and the answer to that
// press names the boxes the copy used, so the page can empty them and leave the rest alone.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const theDocument = ({ mutable = true } = {}) => [base('p', 'tab', { mutable: { Boolean: mutable } }, [
  base('NewTask', 'div', { layout: { String: 'horizontal' } }, [
    base('title', 'textbox', { value: { String: '' }, placeholder: { String: 'what' } }),
    base('points', 'textbox', { value: { String: '1' } }),
    base('add', 'button', { label: { String: 'add' } }, [
      base('click', 'on', {}, [base('append', 'append', { list: { String: '/p/Items' }, from: { String: '..' } })]),
    ]),
    base('labels', 'textbox', { value: { String: '' }, vocabulary: { String: '/p/Labels' }, placeholder: { String: 'tags' } }),
  ]),
  base('Labels', 'list', {}, ['ui', 'dsl'].map((tag, i) => base(`Label__${i + 1}`, 'div', {}, [
    base('tag', 'string', { value: { String: tag } }),
    base('name', 'string', { value: { String: tag } }),
  ]))),
  base('Elsewhere', 'div', {}, [
    base('note', 'textbox', { value: { String: '' } }),
  ]),
])]

let app

const render = (options) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(options)
  app.currentFile = 'p.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const box = (...names) => document.querySelector(`[data-path='${JSON.stringify(['p', ...names])}'] input.textbox-input`)

const type = (input, text) => {
  input.value = text
  input.dispatchEvent(new window.Event('input', { bubbles: true }))
}

const calls = (name) => invoke.mock.calls.filter(([cmd]) => cmd === name)

describe('a textbox', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation(async () => null)
  })

  it('is a box to type into, showing what it starts with', () => {
    render()
    expect(box('NewTask', 'title')).not.toBeNull()
    expect(box('NewTask', 'title').placeholder).toBe('what')
    expect(box('NewTask', 'points').value).toBe('1')
    expect(box('NewTask', 'points').readOnly).toBe(false)
  })

  it('sends nothing while it is typed into', () => {
    render()
    type(box('NewTask', 'title'), 'Write the docs')
    expect(invoke.mock.calls, 'typing reached the backend').toEqual([])
  })

  it('keeps what was typed through a repaint', () => {
    render()
    type(box('NewTask', 'title'), 'Write the docs')
    app.renderer.renderDocument(app.currentDocument)
    expect(box('NewTask', 'title').value).toBe('Write the docs')
  })

  it('cannot be typed into where nothing may change', () => {
    render({ mutable: false })
    expect(box('NewTask', 'title').readOnly).toBe(true)
  })
})

describe('a press with a form on the page', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
  })

  const answering = (emptied) => invoke.mockImplementation(async (cmd) =>
    cmd === 'run_overseer_event'
      ? { text: 'TEXT-2', changes: [], nodes: null, wrote: true, emptied }
      : null)

  const pressAdd = async () => {
    await app.renderer.emitEvent(
      app.currentDocument[0].children[0].children[2],
      document.querySelector(`[data-path='${JSON.stringify(['p', 'NewTask', 'add'])}']`),
      'click')
  }

  it('carries what is typed, each path said once', async () => {
    render()
    answering([])
    type(box('NewTask', 'title'), 'Write the docs')
    type(box('NewTask', 'points'), '3')
    await pressAdd()

    const [[, sent]] = calls('run_overseer_event')
    const byPath = Object.fromEntries(sent.typed.map((t) => [t.node_path.join('/'), t.value]))
    expect(byPath).toEqual({ 'p/NewTask/title': { String: 'Write the docs' }, 'p/NewTask/points': { String: '3' } })
    for (const t of sent.typed) expect(Object.keys(t).sort()).toEqual(['node_path', 'value'])
  })

  it('empties the boxes the press used, and only those', async () => {
    render()
    answering([['p', 'NewTask', 'title'], ['p', 'NewTask', 'points']])
    type(box('NewTask', 'title'), 'Write the docs')
    type(box('NewTask', 'points'), '3')
    type(box('Elsewhere', 'note'), 'half written')
    await pressAdd()

    expect(box('NewTask', 'title').value).toBe('')
    expect(box('NewTask', 'points').value, 'it did not go back to what it starts with').toBe('1')
    expect(box('Elsewhere', 'note').value, 'a box the press did not use was emptied').toBe('half written')

    // And a later press no longer carries what was emptied.
    invoke.mockClear()
    answering([])
    await pressAdd()
    const [[, sent]] = calls('run_overseer_event')
    expect(sent.typed.map((t) => t.node_path.join('/'))).toEqual(['p/Elsewhere/note'])
  })
})

describe('a textbox with a vocabulary', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation(async () => null)
  })

  const field = () => document.querySelector(`[data-path='${JSON.stringify(['p', 'NewTask', 'labels'])}']`)
  const chips = () => Array.from(field().querySelectorAll('.tag-chip:not(.tag-option)')).map((c) => c.dataset.tag)
  const pick = (tag) => {
    field().querySelector('.tag-add').click()
    Array.from(document.querySelectorAll('.tag-option')).find((o) => o.dataset.tag === tag).click()
  }

  it('picks tags the way a tags field does, saying what goes there while it is empty', () => {
    render()
    expect(field().classList.contains('tags-field')).toBe(true)
    expect(field().querySelector('.tag-placeholder').textContent).toBe('tags')
    pick('ui')
    pick('dsl')
    expect(chips()).toEqual(['ui', 'dsl'])
    expect(invoke.mock.calls, 'a pick reached the backend').toEqual([])
  })

  it('sends what was picked with a press, and is emptied like any box', async () => {
    render()
    pick('ui')
    pick('dsl')
    invoke.mockImplementation(async (cmd) =>
      cmd === 'run_overseer_event'
        ? { text: 'TEXT-2', changes: [], nodes: null, wrote: true, emptied: [['p', 'NewTask', 'labels']] }
        : null)
    await app.renderer.emitEvent(
      app.currentDocument[0].children[0].children[2],
      document.querySelector(`[data-path='${JSON.stringify(['p', 'NewTask', 'add'])}']`),
      'click')

    const [[, sent]] = calls('run_overseer_event')
    expect(sent.typed).toEqual([{ node_path: ['p', 'NewTask', 'labels'], value: { String: 'ui, dsl' } }])
    expect(chips(), 'the picks were not emptied').toEqual([])
  })
})
