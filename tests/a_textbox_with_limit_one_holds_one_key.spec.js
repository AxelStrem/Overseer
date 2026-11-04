import { describe, it, expect, beforeEach, vi } from 'vitest'

// A record names one food, and a picking textbox held a set: after the first pick it went on
// offering `+`. With `limit=1` it holds one key - picking another replaces it, the way an enum's
// chip is changed - and `+` comes back only once the box is emptied.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})
const text = (name, value) => base(name, 'string', { value: { String: value } })

const theDocument = ({ limit = 1, mutable = true } = {}) => [base('p', 'tab', { mutable: { Boolean: mutable } }, [
  base('Foods', 'list', { key: { String: 'handle' } }, ['apple', 'bread', 'rice'].map((handle) =>
    base(`Food__${handle}`, 'div', {}, [text('handle', handle), text('name', handle.toUpperCase())]))),
  base('food', 'textbox', {
    value: { String: '' }, vocabulary: { String: '/p/Foods' }, limit: { Integer: limit },
  }),
])]

let app
const render = (options) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(options)
  app.currentFile = 'p.os'
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

const box = () => document.querySelector('.textbox-field')
const plus = () => box().querySelector('.tag-add')
const held = () => Array.from(box().querySelectorAll('.tag-chip')).map(c => c.dataset.tag)
const offer = (tag) => document.querySelector(`.tag-picker .tag-option[data-tag="${tag}"]`)
const typed = () => [...app.renderer._typed.values()]

describe('a textbox with limit=1', () => {
  beforeEach(() => setupDOM())

  it('offers `+` until one key is picked, and not after', () => {
    render()
    plus().click()
    offer('bread').click()
    expect(held()).toEqual(['bread'])
    expect(plus()).toBeNull()
    expect(typed()).toEqual(['bread'])
  })

  it('replaces the key it holds with the one picked from its chip', () => {
    render()
    plus().click()
    offer('bread').click()
    box().querySelector('.tag-chip').click()
    expect(offer('bread')).toBeNull()
    offer('rice').click()
    expect(held()).toEqual(['rice'])
    expect(typed()).toEqual(['rice'])
  })

  it('is emptied by removing its chip, and offers `+` again', () => {
    render()
    plus().click()
    offer('apple').click()
    box().querySelector('.tag-remove').click()
    expect(held()).toEqual([])
    expect(plus()).not.toBeNull()
  })

  it('holds as many as a larger limit says', () => {
    render({ limit: 2 })
    plus().click()
    offer('apple').click()
    plus().click()
    offer('rice').click()
    expect(held()).toEqual(['apple', 'rice'])
    expect(plus()).toBeNull()
  })
})
