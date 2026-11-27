import { describe, it, expect, beforeEach, vi } from 'vitest'

// A typed textbox that says `on change` is pressed once its text is committed. A box that picks
// from a vocabulary has no Enter to press: each pick is what commits it, so each pick presses it,
// and what it holds by then goes along with the press.

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

const theDocument = ({ handled = true } = {}) => [base('p', 'tab', { mutable: { Boolean: true } }, [
  base('Foods', 'list', { key: { String: 'handle' } }, ['apple', 'bread'].map((handle) =>
    base(`Food__${handle}`, 'div', {}, [text('handle', handle)]))),
  text('picked', ''),
  base('food', 'textbox', {
    value: { String: '' }, vocabulary: { String: '/p/Foods' }, limit: { Integer: 1 },
  }, handled ? [base('change', 'on', {}, [base('set', 'set', { path: { String: '../picked' } })])] : []),
])]

let app
let pressed
const render = (options) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(options)
  app.currentFile = 'p.os'
  app._currentText = 'TEXT'
  pressed = []
  app.renderer.emitEvent = vi.fn(async (node, element, eventName) => {
    pressed.push({ name: node.name, eventName, holding: [...app.renderer._typed.values()] })
  })
  app.renderer.renderDocument(app.currentDocument)
}

const box = () => document.querySelector('.textbox-field')
const offer = (tag) => document.querySelector(`.tag-picker .tag-option[data-tag="${tag}"]`)

describe('a picking textbox that says on change', () => {
  beforeEach(() => setupDOM())

  it('is pressed by a pick, holding what was picked', () => {
    render()
    box().querySelector('.tag-add').click()
    offer('bread').click()
    expect(pressed).toEqual([{ name: 'food', eventName: 'change', holding: ['bread'] }])
  })

  it('is pressed again by a pick that replaces what it held', () => {
    render()
    box().querySelector('.tag-add').click()
    offer('apple').click()
    box().querySelector('.tag-chip').click()
    offer('bread').click()
    expect(pressed.map((p) => p.holding)).toEqual([['apple'], ['bread']])
  })

  it('is not pressed when it says no on change', () => {
    render({ handled: false })
    box().querySelector('.tag-add').click()
    offer('bread').click()
    expect(pressed).toEqual([])
  })
})
