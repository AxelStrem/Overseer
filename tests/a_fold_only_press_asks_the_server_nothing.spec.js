import { describe, it, expect, beforeEach, vi } from 'vitest'

// A press that only folds - every action of its `on click` a `fold`, `unfold` or `toggle_fold`
// with a written-out path to a drawn div - is done in the page: it changes nothing in the file,
// and the server's answer only said which div to fold. Anything else still goes to the server.

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

// t / Open / Task__n / [ key, (talk, notes, nowhere, both, figured), Talk, Notes ]
const task = (n, key) => base(`Task__${n}`, 'div', {}, [
  base('key', 'string', { value: { String: key }, hidden: { Boolean: true } }),
  base('div', 'div', { layout: { String: 'horizontal' } }, [
    opener('talk', 'toggle_fold', '../Talk'),
    opener('notes', 'unfold', '../Notes'),
    opener('nowhere', 'toggle_fold', '../Elsewhere'),
    base('both', 'button', { label: { String: 'both' } }, [
      base('click', 'on', {}, [
        base('toggle_fold', 'toggle_fold', { path: { String: '../Talk' } }),
        base('set', 'set', { path: { String: '../key' } }),
      ]),
    ]),
    base('figured', 'button', { label: { String: 'figured' } }, [
      base('click', 'on', {}, [base('toggle_fold', 'toggle_fold', { path: { Formula: '"../Talk"' } })]),
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
  app.renderer.renderDocument(app.currentDocument)
}

const drawn = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', 'Open', ...path])}']`)
const button = (entry, name) => {
  const el = drawn(entry, name)
  return el && (el.matches('button') ? el : el.querySelector('button'))
}
const press = async (entry, name) => {
  button(entry, name).click()
  await new Promise((r) => setTimeout(r, 0))
}
const folded = (entry, div) => drawn(entry, div)?.classList.contains('folded')
const shown = (entry, div) => !!drawn(entry, div) && !folded(entry, div)
const asked = () => invoke.mock.calls.filter(([command]) => command === 'run_overseer_event').length

describe('a press that only folds', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
    invoke.mockImplementation(async () => null)
  })

  it('folds and unfolds its div in the page and asks the server nothing', async () => {
    open()
    expect(shown('Task__1', 'Talk'), 'Talk starts open').toBe(false)
    await press('Task__1', 'talk')
    expect(shown('Task__1', 'Talk'), 'Talk did not open').toBe(true)
    expect(shown('Task__2', 'Talk'), 'another row opened').toBe(false)
    await press('Task__1', 'talk')
    expect(shown('Task__1', 'Talk'), 'Talk did not fold again').toBe(false)
    expect(invoke).not.toHaveBeenCalled()
  })

  it('leaves an open div open when it unfolds', async () => {
    open()
    await press('Task__1', 'notes')
    expect(shown('Task__1', 'Notes')).toBe(true)
    expect(invoke).not.toHaveBeenCalled()
  })

  it('goes to the server when it does something besides folding', async () => {
    open()
    await press('Task__1', 'both')
    expect(asked()).toBe(1)
  })

  it('goes to the server when its div is named by a formula', async () => {
    open()
    await press('Task__1', 'figured')
    expect(asked()).toBe(1)
  })

  it('goes to the server when its div cannot be found', async () => {
    open()
    await press('Task__1', 'nowhere')
    expect(asked()).toBe(1)
  })
})
