import { describe, it, expect, beforeEach, vi } from 'vitest'

// A foldable div that says `float=true` is drawn over the page while it is open, anchored to the
// button that opened it, and folds again on a press anywhere outside it or on Escape. Its fold is
// the viewer's for as long as the page is open, and is not kept past a reload.

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

// t / Open / Task__n / [ key, (more), menu (floating), after ]
const task = (n, key) => base(`Task__${n}`, 'div', {}, [
  base('key', 'string', { value: { String: key }, hidden: { Boolean: true } }),
  base('div', 'div', { layout: { String: 'horizontal' } }, [opener('more', 'toggle_fold', '../menu')], true),
  base('menu', 'div', { foldable: { Boolean: true }, folded: { Boolean: true }, float: { Boolean: true } }, [
    base('note', 'string', { value: { String: `menu of ${key}` } }),
  ]),
  base('after', 'string', { value: { String: `after ${key}` } }),
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
const button = (entry) => {
  const el = drawn(entry, 'more')
  return el && (el.matches('button') ? el : el.querySelector('button'))
}
const press = async (entry) => {
  button(entry).dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }))
  button(entry).dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await new Promise((r) => setTimeout(r, 0))
}
const menu = (entry) => drawn(entry, 'menu')
const pointAt = (el) => el.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }))

describe('a floating div', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
  })

  it('is opened by its button and drawn over the page', async () => {
    open()
    expect(menu('Task__1'), 'drawn before it was opened').toBeNull()
    await press('Task__1')
    expect(menu('Task__1'), 'not drawn once opened').not.toBeNull()
    expect(menu('Task__1').classList.contains('overseer-floating')).toBe(true)
    expect(menu('Task__1').textContent).toContain('menu of a')
    expect(menu('Task__2'), 'another entry opened too').toBeNull()
    expect(button('Task__1').classList.contains('toggled'), 'its button not drawn pressed').toBe(true)
  })

  it('folds on a press outside it, and stays open on a press inside it', async () => {
    open()
    await press('Task__1')
    pointAt(menu('Task__1').querySelector('*') || menu('Task__1'))
    expect(menu('Task__1'), 'folded by a press inside it').not.toBeNull()
    pointAt(drawn('Task__2', 'after'))
    expect(menu('Task__1'), 'still open after a press outside it').toBeNull()
  })

  it('folds when its button is pressed again', async () => {
    open()
    await press('Task__1')
    await press('Task__1')
    expect(menu('Task__1')).toBeNull()
  })

  it('folds when another one is opened', async () => {
    open()
    await press('Task__1')
    await press('Task__2')
    expect(menu('Task__1'), 'the first stayed open').toBeNull()
    expect(menu('Task__2'), 'the second did not open').not.toBeNull()
  })

  it('folds on Escape', async () => {
    open()
    await press('Task__2')
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(menu('Task__2')).toBeNull()
  })

  it('is not kept past a reload', async () => {
    open()
    await press('Task__1')
    const kept = sessionStorage.getItem('overseer.folds:t.os')
    expect(kept === null || !kept.includes('menu'), `kept: ${kept}`).toBe(true)
    open()
    expect(menu('Task__1'), 'came back open in a new page').toBeNull()
  })
})
