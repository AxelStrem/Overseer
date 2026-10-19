import { describe, it, expect, beforeEach, vi } from 'vitest'

// A grid holds `columns` x `rows` cells, which the resolver gives row by row and names by their
// place, `A1`, `B1`, ... The page draws them in that order in as many columns, with the column
// letters above and the row numbers to the left unless the grid says `header=false`. A cell
// written for a place outside the grid is kept in the file, and has nowhere to be drawn.

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

const gridOf = (columns, rows, { header, extra = [] } = {}) => {
  const parameters = {
    entry: { String: 'int' }, columns: { Integer: columns }, rows: { Integer: rows },
  }
  if (header !== undefined) parameters.header = { Boolean: header }
  const children = []
  for (let r = 1; r <= rows; r++) {
    for (let c = 0; c < columns; c++) children.push(cell(String.fromCharCode(65 + c) + r, r * 10 + c))
  }
  return [Object.assign(base('t', 'tab'), {
    children: [Object.assign(base('Scores', 'grid'), { parameters, children: [...children, ...extra] })],
  })]
}

const grid = () => document.querySelector('.overseer-grid')
const headings = (kind) => Array.from(grid().querySelectorAll(`:scope > .overseer-grid-heading.${kind}`))
  .map(h => h.textContent)
const drawnCells = () => Array.from(grid().children)
  .filter(el => el.dataset && el.dataset.path)
  .map(el => JSON.parse(el.dataset.path).slice(-1)[0])

describe('a grid', () => {
  const render = (doc) => {
    const app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('draws its cells row by row in as many columns as it has', () => {
    render(gridOf(3, 2))
    expect(drawnCells()).toEqual(['A1', 'B1', 'C1', 'A2', 'B2', 'C2'])
    expect(grid().style.gridTemplateColumns).toBe('auto repeat(3, minmax(0, 1fr))')
  })

  it('heads its columns with letters and its rows with numbers, each row after its number', () => {
    render(gridOf(3, 2))
    expect(headings('column')).toEqual(['A', 'B', 'C'])
    expect(headings('row')).toEqual(['1', '2'])
    const order = Array.from(grid().children).map(el =>
      el.classList.contains('overseer-grid-heading') ? `#${el.textContent}` : JSON.parse(el.dataset.path).slice(-1)[0])
    expect(order).toEqual(['#', '#A', '#B', '#C', '#1', 'A1', 'B1', 'C1', '#2', 'A2', 'B2', 'C2'])
  })

  it('draws no headings when it says header=false', () => {
    render(gridOf(2, 2, { header: false }))
    expect(grid().querySelectorAll('.overseer-grid-heading')).toHaveLength(0)
    expect(grid().style.gridTemplateColumns).toBe('repeat(2, minmax(0, 1fr))')
  })

  it('does not draw a cell written for a place outside it', () => {
    render(gridOf(2, 1, { extra: [cell('C3', 1), cell('note', 2)] }))
    expect(drawnCells()).toEqual(['A1', 'B1'])
  })
})
