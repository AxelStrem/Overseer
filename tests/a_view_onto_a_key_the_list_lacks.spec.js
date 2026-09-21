import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

// A view onto a list is parameterised by a key. Where no entry has that key the view shows the
// template as a preview - it looks like an entry, but the document has none. The moment a field
// in that view is edited the entry becomes real, at that key, and the edit lands on it; from then
// on it is an ordinary view onto an ordinary entry.
//
// The page used to do the making itself, in its own copy of the document, and leave the whole
// text to be saved over the file. It asks for it now, so what these check is the asking: that
// nothing is asked for by looking, that an edit asks for the right entry in the right place and
// carries what was typed, and that the answer is taken on rather than saved over.
//
// What happens to the file when it is asked is `a_preview_becomes_real.rs`, through the same
// door - including that the entry is made once, that a write made in the meantime survives, and
// that a day created this way freezes what it was aiming at.

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
import { answerEnsureEntry } from './helpers/ensure-entry.js'

const clone = (o) => JSON.parse(JSON.stringify(o))

const findByPath = (wanted) => {
  for (const el of Array.from(document.querySelectorAll('[data-path]'))) {
    try {
      const p = JSON.parse(el.dataset.path || '[]')
      if (p.length === wanted.length && p.every((v, i) => v === wanted[i])) return el
    } catch (_) {}
  }
  return null
}

/// A history keyed by date, a template, and a view onto whichever date is selected. Nothing in
/// the history holds the date the view is pointed at.
const aTracker = (policy, showing = '2026-08-05') => [{
  name: 'tracker', node_type: 'tab', parameters: { mutable: { Boolean: true } }, is_hierarchy_transparent: false,
  children: [
    { name: 'DayRecord', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
      { name: 'date', node_type: 'timestamp', parameters: { precision: { String: 'day' }, value: { String: '' } }, children: [], is_hierarchy_transparent: false },
      { name: 'weight', node_type: 'float', parameters: { mutable: { Boolean: true } }, children: [], is_hierarchy_transparent: false },
      { name: 'mood', node_type: 'string', parameters: { value: { String: '' }, mutable: { Boolean: true } }, children: [], is_hierarchy_transparent: false },
    ]},
    { name: 'Selected', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
      { name: 'showing', node_type: 'timestamp', parameters: { precision: { String: 'day' }, value: { String: showing } }, children: [], is_hierarchy_transparent: false },
      { name: 'SelectedDay', node_type: 'div', is_hierarchy_transparent: false, children: [],
        parameters: {
          link: { String: '/tracker/History[key=$(../showing)]' },
          'phantom-materialize': { String: policy },
          mutable: { Boolean: true },
        }},
    ]},
    { name: 'History', node_type: 'list', is_hierarchy_transparent: false,
      parameters: { entry: { Template: 'DayRecord' }, key: { String: 'date' }, keyPrecision: { String: 'day' } },
      children: [
        { name: 'DayRecord__1', node_type: 'div', parameters: { _from_template: true }, is_hierarchy_transparent: false, children: [
          { name: 'date', node_type: 'timestamp', parameters: { value: { String: '2026-08-03' } }, children: [], is_hierarchy_transparent: false },
          { name: 'weight', node_type: 'float', parameters: { value: { Float: 92.3 } }, children: [], is_hierarchy_transparent: false },
        ]},
        { name: 'DayRecord__2', node_type: 'div', parameters: { _from_template: true }, is_hierarchy_transparent: false, children: [
          { name: 'date', node_type: 'timestamp', parameters: { value: { String: '2026-08-04' } }, children: [], is_hierarchy_transparent: false },
          { name: 'weight', node_type: 'float', parameters: { value: { Float: 93.7 } }, children: [], is_hierarchy_transparent: false },
        ]},
      ]},
  ],
}]

/// The backend, as `helpers/ensure-entry.js` mirrors it: the entry appears at the key with the
/// template's fields and the edit applied, and the list comes back as a subtree.
const aBackendThatMakesTheEntry = (app) => {
  invoke.mockImplementation((cmd, args) => answerEnsureEntry(() => app, cmd, args)
    ?? Promise.resolve(null))
}

/// The same, plus the ordinary value write and a re-resolve that answers from the document as it
/// now stands - so a commit that carries on past the making finishes the way it would in the app
/// rather than against a backend that says nothing.
const aBackendThatAlsoWrites = (app) => {
  invoke.mockImplementation((cmd, args) => {
    const madeReal = answerEnsureEntry(() => app, cmd, args)
    if (madeReal !== null) return madeReal
    if (cmd === 'write_overseer_values') {
      for (const { node_path: path, value } of args.values || []) {
        let at = { children: app.currentDocument }
        for (const name of path) at = (at.children || []).find((c) => c?.name === name) || {}
        if (at.parameters) at.parameters.value = value
      }
      return Promise.resolve({ wrote: true, text: 'AFTER THE WRITE', changes: [], nodes: null })
    }
    if (cmd.startsWith('parse_overseer_content')) {
      return Promise.resolve({ nodes: clone(app.currentDocument), text: 'AFTER THE WRITE' })
    }
    if (cmd === 'serialize_overseer_nodes') return Promise.resolve('TEXT')
    return Promise.resolve(null)
  })
}

const openIt = (doc, { file = '/documents/tracker.os' } = {}) => {
  const app = new OverseerApp()
  window.app = app
  app.currentFile = file
  app._currentText = 'TEXT'
  app._originalText = 'TEXT'
  app.currentDocument = clone(doc)
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const theHistory = (app) =>
  app.currentDocument[0].children.find((c) => c.name === 'History').children

const dateOf = (entry) => {
  const v = entry.children.find((c) => c.name === 'date')?.parameters?.value
  return v?.String ?? v?.Timestamp ?? null
}

const datesIn = (app) => theHistory(app).map(dateOf)

const theView = () => {
  const container = findByPath(['tracker', 'Selected', 'SelectedDay'])
  expect(container, 'the view was not rendered').toBeTruthy()
  return container
}

/// A field of the view edited, as the commit path does it.
const materialise = (app, position, value) => {
  const meta = JSON.parse(theView().getAttribute('data-link-phantom') || '{}')
  expect(meta.listPath, 'the view is not offering a phantom').toBeTruthy()
  return app.renderer._materializePhantomAndComputePath(
    Object.assign({}, meta, { tailSegments: ['weight'] }), { position, value })
}

const asked = () => invoke.mock.calls.filter(([cmd]) => cmd === 'ensure_overseer_entry')

describe('a view onto a key the list does not have', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
  })
  afterEach(() => { delete window.app })

  it('shows the template as a preview and asks for nothing', () => {
    const app = openIt(aTracker('append-on-edit'))
    expect(theView().hasAttribute('data-link-phantom'), 'nothing was previewed').toBe(true)
    expect(asked(), 'looking at a missing key asked for an entry').toHaveLength(0)
    expect(datesIn(app)).toEqual(['2026-08-03', '2026-08-04'])
  })

  it('asks for nothing when it is looked at again, or pointed elsewhere', () => {
    const app = openIt(aTracker('append-on-edit'))
    app.renderer.renderDocument(app.currentDocument)
    const showing = app.currentDocument[0].children
      .find((c) => c.name === 'Selected').children.find((c) => c.name === 'showing')
    showing.parameters.value = { String: '2026-08-06' }
    app.renderer.renderDocument(app.currentDocument)

    expect(asked()).toHaveLength(0)
    expect(datesIn(app)).toEqual(['2026-08-03', '2026-08-04'])
  })

  it('asks for the entry at the key it is pointed at', async () => {
    const app = openIt(aTracker('append-on-edit'))
    aBackendThatMakesTheEntry(app)
    await materialise(app, 'append', { Float: 91 })

    expect(asked(), 'the edit did not ask for the entry').toHaveLength(1)
    const [, args] = asked()[0]
    expect(args.path).toBe('/documents/tracker.os')
    expect(args.wanted.list_path).toEqual(['tracker', 'History'])
    expect(args.wanted.key_field).toBe('date')
    expect(args.wanted.key_value).toEqual({ String: '2026-08-05' })
    expect(args.wanted.template).toBe('DayRecord')
  })

  it('names each thing once', async () => {
    // The older commands take their arguments at the top level, where the backend reads either
    // spelling and sending both is harmless. These go inside one object, read as a struct whose
    // fields take the other spelling as an alias - and both at once is a duplicate field, which
    // loses the whole request. Nothing about that is visible from here, which is why it is
    // pinned here as well as on the other side.
    const app = openIt(aTracker('append-on-edit'))
    aBackendThatMakesTheEntry(app)
    await materialise(app, 'append', { Float: 91 })

    const names = Object.keys(asked()[0][1].wanted)
    const camel = names.filter((n) => /[A-Z]/.test(n))
    expect(camel, `sent in two spellings: ${names.join(', ')}`).toEqual([])
    expect(names.sort()).toEqual(
      ['fields', 'key_field', 'key_value', 'list_path', 'position', 'template'])
  })

  it('carries the edit with it, so the two are one change', async () => {
    // Making the entry and writing what was typed is one thing a person did. Asked for
    // separately they would be two file writes and two presses of Undo.
    const app = openIt(aTracker('append-on-edit'))
    aBackendThatMakesTheEntry(app)
    await materialise(app, 'append', { Float: 91 })

    expect(asked()[0][1].wanted.fields).toEqual({ weight: { Float: 91 } })
  })

  it('says where the entry should go, as the document asked', async () => {
    const app = openIt(aTracker('prepend-on-edit'))
    aBackendThatMakesTheEntry(app)
    await materialise(app, 'prepend', { Float: 91 })

    expect(asked()[0][1].wanted.position).toBe('prepend')
    expect(datesIn(app), 'the answer was not taken on')
      .toEqual(['2026-08-05', '2026-08-03', '2026-08-04'])
  })

  it('takes the answer on and leaves no save behind to repeat it', async () => {
    // Written as part of making it. A save left to fire would send the whole document as text -
    // which is what this replaced - and would be a second step to take back.
    vi.useFakeTimers()
    try {
      const app = openIt(aTracker('append-on-edit'))
      aBackendThatMakesTheEntry(app)
      app.markDocumentModified()
      await materialise(app, 'append', { Float: 91 })
      await vi.runAllTimersAsync()

      expect(datesIn(app)).toContain('2026-08-05')
      expect(app._originalText, 'the page kept a baseline the file no longer has')
        .toBe('AFTER THE WRITE')
      const saved = invoke.mock.calls.filter(([cmd]) => cmd.startsWith('save_overseer_file'))
      expect(saved, 'the document was sent as text as well').toHaveLength(0)
    } finally {
      vi.useRealTimers()
    }
  })

  it('answers with the path of the entry that now holds the key', async () => {
    // Found by its key rather than by where it went, because a keyed list is written at either
    // end and which end is the document's business.
    const app = openIt(aTracker('prepend-on-edit'))
    aBackendThatMakesTheEntry(app)
    const path = await materialise(app, 'prepend', { Float: 91 })

    expect(path).toBe('tracker/History/DayRecord__3/weight')
    const entry = theHistory(app).find((e) => dateOf(e) === '2026-08-05')
    expect(entry.name).toBe('DayRecord__3')
  })

  it('leaves no save behind when the edit goes through the commit path', async () => {
    // Not the same as calling the materialisation directly. The commit carries on afterwards,
    // and three of its branches used to mark the document modified and return - leaving the
    // save to carry the change, because back then the save was the only thing that would. The
    // entry and the edit are written as part of being made now, so a save left scheduled would
    // send the whole document to repeat what the file already says.
    vi.useFakeTimers()
    try {
      const app = openIt(aTracker('append-on-edit'))
      aBackendThatAlsoWrites(app)

      const shown = theView().querySelector('.string-field .field-value')
      expect(shown, 'the preview has no field to edit').toBeTruthy()
      shown.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
      const input = document.querySelector('input.field-editor, textarea.field-editor')
      expect(input, 'the editor did not open').toBeTruthy()
      input.value = 'tired'
      input.dispatchEvent(new window.FocusEvent('blur'))
      await vi.runAllTimersAsync()

      expect(asked(), 'the edit did not ask for the entry').toHaveLength(1)
      const saved = invoke.mock.calls.filter(([cmd]) => cmd.startsWith('save_overseer_file'))
      expect(saved, 'the document went up as text as well').toHaveLength(0)
      expect(app.isDocumentModified, 'it still thinks something is unsaved').toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })

  it('asks for nothing when the document has no address to write to', async () => {
    // Every other change the page makes needs a file to reach; so does this. Making the entry
    // in the page's own copy instead is what this replaced, and keeping that as a second path
    // is how the two would drift apart.
    const app = openIt(aTracker('append-on-edit'), { file: null })
    aBackendThatMakesTheEntry(app)
    const path = await materialise(app, 'append', { Float: 91 })

    expect(asked()).toHaveLength(0)
    expect(path).toBeNull()
  })
})
