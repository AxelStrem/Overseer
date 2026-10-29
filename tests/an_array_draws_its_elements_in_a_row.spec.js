import { describe, it, expect, beforeEach, vi } from 'vitest'

// An array is one row of a grid: `length` elements, named by the column letters alone, `A`,
// `B`, ... The page draws them in a row headed by their letters, or with `layout="vertical"` in a
// column with the letters to the left; `header=false` leaves the letters out. An element written
// past the end is kept in the file, and has nowhere to be drawn.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const cell = (name, value) => Object.assign(base(name, 'int'), {
  parameters: { value: { Integer: value } },
})

const arrayOf = (length, { header, layout, extra = [] } = {}) => {
  const parameters = { entry: { String: 'int' }, length: { Integer: length } }
  if (header !== undefined) parameters.header = { Boolean: header }
  if (layout !== undefined) parameters.layout = { String: layout }
  const children = []
  for (let c = 0; c < length; c++) children.push(cell(String.fromCharCode(65 + c), c))
  return [Object.assign(base('t', 'tab'), {
    children: [Object.assign(base('Week', 'array'), { parameters, children: [...children, ...extra] })],
  })]
}

const array = () => document.querySelector('.overseer-grid')
const order = () => Array.from(array().children).map(el =>
  el.classList.contains('overseer-grid-heading') ? `#${el.textContent}` : JSON.parse(el.dataset.path).slice(-1)[0])

describe('an array', () => {
  const render = (doc) => {
    const app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('draws its elements in one row, headed by their letters', () => {
    render(arrayOf(3))
    expect(order()).toEqual(['#A', '#B', '#C', 'A', 'B', 'C'])
    expect(array().style.gridTemplateColumns).toBe('repeat(3, minmax(0, 1fr))')
  })

  it('draws them in one column, each after its letter, when it says layout="vertical"', () => {
    render(arrayOf(3, { layout: 'vertical' }))
    expect(order()).toEqual(['#A', 'A', '#B', 'B', '#C', 'C'])
    expect(array().style.gridTemplateColumns).toBe('auto minmax(0, 1fr)')
  })

  it('draws no headings when it says header=false', () => {
    render(arrayOf(2, { header: false }))
    expect(order()).toEqual(['A', 'B'])
    render(arrayOf(2, { header: false, layout: 'vertical' }))
    expect(order()).toEqual(['A', 'B'])
    expect(array().style.gridTemplateColumns).toBe('minmax(0, 1fr)')
  })

  it('does not draw an element written past its end', () => {
    render(arrayOf(2, { extra: [cell('D', 1), cell('A1', 2), cell('note', 3)] }))
    expect(order()).toEqual(['#A', '#B', 'A', 'B'])
  })
})
