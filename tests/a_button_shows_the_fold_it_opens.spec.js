import { describe, it, expect, beforeEach, vi } from 'vitest'

// A button that opens a div - `toggle_fold` or `unfold` in its `on click` - is drawn pressed
// while the div is open, so a row says which of its sections are showing. A fold redraws only
// the div, so the button, which sits outside it, is brought up to date on its own; and the state
// comes from the page's folds, so it holds across a redraw.

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

const opener = (name, action, path) => base(name, 'button', { label: { String: name } }, [
  base('click', 'on', {}, [base(action, action, { path: { String: path } })]),
])

// t / Open / Task__n / [ key, (talk, notes, other), Talk, Notes ]
const task = (n, key) => base(`Task__${n}`, 'div', {}, [
  base('key', 'string', { value: { String: key }, hidden: { Boolean: true } }),
  base('div', 'div', { layout: { String: 'horizontal' } }, [
    opener('talk', 'toggle_fold', '../Talk'),
    opener('notes', 'unfold', '../Notes'),
    base('other', 'button', { label: { String: 'other' } }, [
      base('click', 'on', {}, [base('set', 'set', { path: { String: '../key' } })]),
    ]),
  ], true),
  base('Talk', 'div', { foldable: { Boolean: true }, folded: { Boolean: true } }, [
    base('said', 'string', { value: { String: `said about ${key}` } }),
  ]),
  base('Notes', 'div', { foldable: { Boolean: true }, label: { String: 'Notes' } }, [
    base('note', 'string', { value: { String: `noted about ${key}` } }),
  ]),
])

const theDocument = () => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Open', 'list', { key: { String: 'key' } }, [task(1, 'a'), task(2, 'b')]),
])]

let app

const open = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const drawn = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', 'Open', ...path])}']`)
const button = (entry, name) => {
  const el = drawn(entry, name)
  return el && (el.matches('button') ? el : el.querySelector('button'))
}
const pressed = (entry, name) => button(entry, name)?.classList.contains('toggled')
const entry = (n) => app.currentDocument[0].children[0].children[n - 1]
const fold = (n, div, folded) =>
  app.renderer.setFolded(entry(n).children[div === 'Talk' ? 2 : 3], ['t', 'Open', `Task__${n}`, div], folded)

describe('a button that opens a div', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
  })

  it('is drawn unpressed while its div is folded, and pressed while it is open', () => {
    open()
    expect(button('Task__1', 'talk'), 'no talk button was drawn').not.toBeNull()
    expect(pressed('Task__1', 'talk'), 'pressed while Talk is folded').toBe(false)
    expect(button('Task__1', 'talk').getAttribute('aria-pressed')).toBe('false')
    expect(pressed('Task__1', 'notes'), 'unpressed while Notes is open').toBe(true)
  })

  it('follows its div when it opens and when it folds again, and no other row does', () => {
    open()
    fold(1, 'Talk', false)
    expect(pressed('Task__1', 'talk'), 'unpressed after Talk opened').toBe(true)
    expect(pressed('Task__2', 'talk'), 'another row followed').toBe(false)
    fold(1, 'Talk', true)
    expect(pressed('Task__1', 'talk'), 'still pressed after Talk folded').toBe(false)
  })

  it('follows a fold made from the div header', () => {
    open()
    drawn('Task__1', 'Notes').querySelector(':scope > .overseer-fold-header')
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(pressed('Task__1', 'notes'), 'still pressed after Notes folded by its header').toBe(false)
  })

  it('keeps its state through a redraw', () => {
    open()
    fold(2, 'Talk', false)
    app.renderer.renderDocument(app.currentDocument)
    expect(pressed('Task__2', 'talk')).toBe(true)
    expect(pressed('Task__1', 'talk')).toBe(false)
  })

  it('leaves a button that opens nothing alone', () => {
    open()
    expect(pressed('Task__1', 'other')).toBe(false)
    expect(button('Task__1', 'other').hasAttribute('aria-pressed')).toBe(false)
  })
})
