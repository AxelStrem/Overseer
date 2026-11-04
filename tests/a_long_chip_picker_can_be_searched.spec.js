import { describe, it, expect, beforeEach, vi } from 'vitest'

// A picker laid out every chip of its vocabulary at once: fine for a project's labels, useless
// for a catalogue of a hundred foods. Past a dozen, it opens with a box to find one in - typed
// words narrow the chips, the arrows and Enter pick - and a picking textbox can say, with `by`,
// which fields of the catalogue's entries are looked in.

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

const FOODS = [
  ['apple', 'Apple', 'green'], ['banana', 'Banana', ''], ['bread', 'Bread', 'rye'],
  ['butter', 'Butter', ''], ['cheese', 'Cheese', 'cheddar'], ['chicken', 'Chicken', 'breast'],
  ['egg', 'Egg', 'boiled'], ['milk', 'Milk', 'oat'], ['oats', 'Oats', ''], ['pasta', 'Pasta', ''],
  ['rice', 'Rice', 'brown'], ['salmon', 'Salmon', ''], ['tomato', 'Tomato', ''], ['yogurt', 'Yogurt', 'greek'],
]

const theDocument = ({ foods = FOODS, by = null } = {}) => [base('p', 'tab', { mutable: { Boolean: true } }, [
  base('Foods', 'list', { key: { String: 'handle' } }, foods.map(([handle, name, sub]) => base(`Food__${handle}`, 'div', {}, [
    text('handle', handle), text('name', name), text('sub_name', sub),
  ]))),
  base('eaten', 'textbox', {
    value: { String: '' }, vocabulary: { String: '/p/Foods' },
    ...(by ? { by: { String: by } } : {}),
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

const open = () => document.querySelector('.tag-add').click()
const find = () => document.querySelector('.tag-picker-find')
const offered = () => Array.from(document.querySelectorAll('.tag-picker .tag-option'))
  .filter(o => o.style.display !== 'none').map(o => o.dataset.tag)
const held = () => Array.from(document.querySelectorAll('.textbox-field .tag-chip:not(.tag-option)'))
  .map(c => c.dataset.tag)
const type = (value) => { find().value = value; find().dispatchEvent(new Event('input')) }
const key = (key) => find().dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))

describe('a long chip picker', () => {
  beforeEach(() => setupDOM())

  it('opens with a box to find a chip in, and takes the cursor', () => {
    render()
    open()
    expect(find()).not.toBeNull()
    expect(document.activeElement).toBe(find())
    expect(offered()).toHaveLength(FOODS.length)
  })

  it('has no find box while every chip can be seen at a glance', () => {
    render({ foods: FOODS.slice(0, 5) })
    open()
    expect(document.querySelector('.tag-picker')).not.toBeNull()
    expect(find()).toBeNull()
  })

  it('narrows to the chips every typed word is found in, by key and name', () => {
    render()
    open()
    type('BR')
    expect(offered()).toEqual(['bread'])
    type('b')
    expect(offered()).toEqual(['banana', 'bread', 'butter'])
  })

  it('looks in the fields `by` names, a sub-name included', () => {
    render({ by: 'handle, name, sub_name' })
    open()
    type('gree')
    expect(offered()).toEqual(['apple', 'yogurt'])
    type('breast')
    expect(offered()).toEqual(['chicken'])
  })

  it('picks the lit chip with Enter, the arrows moving the light', () => {
    render()
    open()
    type('b')
    key('ArrowDown')
    key('Enter')
    expect(held()).toEqual(['bread'])
    expect(document.querySelector('.tag-picker')).toBeNull()
  })

  it('closes on Escape, picking nothing', () => {
    render()
    open()
    type('apple')
    key('Escape')
    expect(document.querySelector('.tag-picker')).toBeNull()
    expect(held()).toEqual([])
  })

  it('is not closed by a press in its own find box', () => {
    render()
    open()
    find().click()
    expect(document.querySelector('.tag-picker')).not.toBeNull()
  })
})
