import { describe, it, expect, beforeEach, vi } from 'vitest'

// A div that says `foldable=true` folds away and comes back, in the page and nowhere else.
//
// With a label it draws the label as a header, pressed anywhere to fold or unfold; folded, the
// header stays and what is under it goes. Without one it folds away whole, and only an action
// brings it back. The page keeps the fold per tab - across a redraw and a reload - under the
// div's place, with an entry of a keyed list named by its key, so the fold follows the item. What
// the document says, `folded`, is where it starts; once that reads otherwise, the page lets go.

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

// t / Open / Task__n / [ key, Notes (comment), Extra (more), hide ]
const task = (n, key, folded = false) => base(`Task__${n}`, 'div', {}, [
  base('key', 'string', { value: { String: key }, hidden: { Boolean: true } }),
  base('Notes', 'div', {
    foldable: { Boolean: true },
    label: { String: 'Notes' },
    folded: { Formula: '/t/fold_all' },
    _computed_folded: { Boolean: folded },
  }, [
    base('comment', 'string', { value: { String: `said about ${key}` } }),
  ]),
  base('Extra', 'div', { foldable: { Boolean: true } }, [
    base('more', 'string', { value: { String: `more about ${key}` } }),
  ]),
  base('hide', 'button', { label: { String: 'hide' } }, [
    base('click', 'on', {}, [base('fold', 'fold', { path: { String: '../Extra' } })]),
  ]),
])

const theDocument = (folded = false) => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Open', 'list', { key: { String: 'key' } }, [task(1, 'a', folded), task(2, 'b', folded)]),
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

const drawn = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', 'Open', ...path])}']`)
const header = (entry) => drawn(entry, 'Notes')?.querySelector(':scope > .overseer-fold-header')
const commentShows = (entry) => !!drawn(entry, 'Notes', 'comment')
const press = (entry) => header(entry).dispatchEvent(new MouseEvent('click', { bubbles: true }))

// What the document says about every Notes div, worked out again - a "fold all" turned over.
const documentSays = (folded) => {
  for (const entry of app.currentDocument[0].children[0].children) {
    entry.children[1].parameters._computed_folded = { Boolean: folded }
  }
  app.renderer.renderDocument(app.currentDocument)
}

describe('a foldable div', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
  })

  it('draws its label as a header, open to start with', () => {
    open()
    expect(header('Task__1'), 'no header was drawn').not.toBeNull()
    expect(header('Task__1').textContent).toContain('Notes')
    expect(header('Task__1').getAttribute('aria-expanded')).toBe('true')
    expect(commentShows('Task__1')).toBe(true)
  })

  it('folds away under its header when pressed, and comes back when pressed again', () => {
    open()
    press('Task__1')
    expect(commentShows('Task__1'), 'the folded div still shows what is under it').toBe(false)
    expect(header('Task__1'), 'the header went with the fold').not.toBeNull()
    expect(header('Task__1').getAttribute('aria-expanded')).toBe('false')
    expect(commentShows('Task__2'), 'another item folded too').toBe(true)
    press('Task__1')
    expect(commentShows('Task__1')).toBe(true)
  })

  it('asks the backend nothing', () => {
    open()
    press('Task__1')
    expect(invoke.mock.calls.length).toBe(0)
  })

  it('stays folded through a redraw and a reload', () => {
    open()
    press('Task__2')
    app.renderer.renderDocument(app.currentDocument)
    expect(commentShows('Task__2')).toBe(false)
    setupDOM()
    open()
    expect(commentShows('Task__2'), 'the fold did not survive a reload').toBe(false)
    expect(commentShows('Task__1')).toBe(true)
  })

  it('follows its item when the list is put in another order', () => {
    open()
    press('Task__1')
    // The same two items the other way round, named by their new places.
    const list = app.currentDocument[0].children[0]
    const [a, b] = list.children
    a.name = 'Task__2'
    b.name = 'Task__1'
    list.children = [b, a]
    app.renderer.renderDocument(app.currentDocument)
    expect(commentShows('Task__2'), 'item a came open when it moved').toBe(false)
    expect(commentShows('Task__1'), 'item b took the fold of the place').toBe(true)
  })

  it('starts where the document says, and follows the document once it says otherwise', () => {
    open(theDocument(true))
    expect(commentShows('Task__1'), 'a div the document folds started open').toBe(false)
    press('Task__1')
    expect(commentShows('Task__1')).toBe(true)
    // Fold all is turned off and on again: every div follows, the one opened by hand too.
    documentSays(false)
    expect(commentShows('Task__1')).toBe(true)
    documentSays(true)
    expect(commentShows('Task__1'), 'a fold set by hand outlived a change in the document').toBe(false)
    expect(commentShows('Task__2')).toBe(false)
  })
})

describe('a fold action', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
  })

  const answering = (folds) => invoke.mockImplementation(async (cmd) =>
    cmd === 'run_overseer_event'
      ? { text: 'TEXT', changes: [], nodes: null, wrote: false, folds }
      : null)

  const pressHide = async (index) => {
    const button = app.currentDocument[0].children[0].children[index].children[3]
    await app.renderer.emitEvent(button, drawn(`Task__${index + 1}`, 'hide'), 'click')
  }

  it('folds away a div with no header, whole', async () => {
    open()
    expect(drawn('Task__1', 'Extra')).not.toBeNull()
    answering([{ address: 't/Open/[a]/Extra', path: [0, 0, 0, 2], how: 'fold' }])
    await pressHide(0)
    expect(drawn('Task__1', 'Extra'), 'the div is still drawn').toBeNull()
    expect(drawn('Task__2', 'Extra')).not.toBeNull()
  })

  it('brings it back, and turns a header div over, in the order asked', async () => {
    open()
    answering([{ address: 't/Open/[a]/Extra', path: [0, 0, 0, 2], how: 'fold' }])
    await pressHide(0)
    answering([
      { address: 't/Open/[a]/Extra', path: [0, 0, 0, 2], how: 'unfold' },
      { address: 't/Open/[a]/Notes', path: [0, 0, 0, 1], how: 'toggle' },
      { address: 't/Open/[a]/Notes', path: [0, 0, 0, 1], how: 'toggle' },
      { address: 't/Open/[a]/Notes', path: [0, 0, 0, 1], how: 'toggle' },
    ])
    await pressHide(0)
    expect(drawn('Task__1', 'Extra'), 'unfold did not bring it back').not.toBeNull()
    expect(commentShows('Task__1'), 'three toggles left it open').toBe(false)
  })
})
