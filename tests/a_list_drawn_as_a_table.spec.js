import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// Columns that line up all the way down a list.
//
// Rows line up today only by coincidence - every row carries the same percentage widths - and the
// coincidence breaks wherever a field is hidden, because a hidden field is not rendered at all and
// everything after it slides left to fill the gap. That is the invariant worth testing, and it is
// testable without a layout engine: a cell is asked which column it is in, and a row that lacks a
// field must leave that column empty rather than close it up.
//
// jsdom computes no grid, so nothing here asserts that anything *looks* aligned. What it asserts
// is the placement the browser is given, plus that the stylesheet half is not inert.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        <button id="save-file-btn"></button><button id="reload-file-btn"></button></div>
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

const field = (name, value, { label = '', width } = {}) => {
  const node = Object.assign(base(name, 'string'), {
    parameters: { value: { String: String(value) }, label: { String: label } },
  })
  if (width) node.parameters.width = { CssSize: { Percentage: width } }
  return node
}

/** As the resolver hands it over: the column set the entry template declares. */
const COLUMNS = JSON.stringify([
  { name: 'handle', label: 'id', width: '10%', span: false },
  { name: 'title', label: 'title', width: '50%', span: false },
  { name: 'done', label: 'done', width: '10%', span: false },
  { name: 'points', label: 'pts', width: '10%', span: false },
  { name: 'note', label: '', width: '', span: true },
])

/** A row showing only the fields it was given - the rest are hidden on this row. */
const row = (n, fields) => Object.assign(base(`Row__${n}`, 'div'), {
  parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
  children: fields,
})

const docOf = (rows, listParams = {}) => ([Object.assign(base('t', 'tab'), {
  children: [
    Object.assign(base('Rows', 'list'), {
      parameters: Object.assign({
        view: { String: 'table' },
        _columns: { String: COLUMNS },
        _effective_layout: { String: 'vertical' },
      }, listParams),
      children: rows,
    }),
  ],
})])

/** A parent: has a percentage, has no finish. */
const withDone = (n) => row(n, [
  field('handle', `h${n}`, { label: 'id', width: 10 }),
  field('title', `title ${n}`, { label: 'title', width: 50 }),
  field('done', '50%', { label: 'done', width: 10 }),
  field('points', '3', { label: 'pts', width: 10 }),
])

/** A leaf: no percentage at all, because it has nothing to work one out from. */
const withoutDone = (n) => row(n, [
  field('handle', `h${n}`, { label: 'id', width: 10 }),
  field('title', `title ${n}`, { label: 'title', width: 50 }),
  field('points', '5', { label: 'pts', width: 10 }),
])

const render = (rows, listParams) => {
  const app = new OverseerApp()
  app.currentDocument = docOf(rows, listParams)
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const table = () => document.querySelector('.view-table')
const rowsOf = () => Array.from(table().children).filter(r => !r.classList.contains('table-header'))
/** Which column each cell of a row was put in, by field name. */
const placement = (rowElement) => Object.fromEntries(
  Array.from(rowElement.children).map((cell) => {
    let name = ''
    try { name = JSON.parse(cell.dataset.path || '[]').slice(-1)[0] || '' } catch { /* none */ }
    return [name, cell.style.gridColumn]
  })
)

describe('a list drawn as a table', () => {
  beforeEach(setupDOM)

  it('puts every cell in the column its field was given', () => {
    render([withDone(1)])
    expect(placement(rowsOf()[0]))
      .toEqual({ handle: '1', title: '2', done: '3', points: '4' })
  })

  it('leaves a column empty rather than closing it up', () => {
    // The whole point. `points` is the fourth column on both rows even though the second row has
    // no percentage before it - which is what a flex row could not do, and why the rows read
    // ragged before.
    render([withDone(1), withoutDone(2)])
    const [parent, leaf] = rowsOf()

    expect(placement(parent).points).toBe('4')
    expect(placement(leaf).points, 'the leaf pulled its points into the empty column').toBe('4')
    expect(placement(leaf).done).toBeUndefined()
  })

  it('takes the widths off the cells and gives them to the columns', () => {
    // A percentage left on a grid item is measured against its own column rather than the row,
    // so a cell that kept its width would be a tenth of a tenth.
    render([withDone(1)])
    expect(table().style.gridTemplateColumns).toBe('10% 50% 10% 10%')
    for (const cell of rowsOf()[0].children) expect(cell.style.width).toBe('')
  })

  it('drops the row layout that would otherwise fight the grid', () => {
    // `.layout-horizontal` carries `display: flex !important`. Removing the class rather than
    // outranking it is what keeps every table rule free of `!important`, and so below the rule
    // that hides a filtered row.
    render([withDone(1)])
    expect(table().classList.contains('layout-vertical')).toBe(false)
    expect(rowsOf()[0].classList.contains('layout-horizontal')).toBe(false)
    expect(rowsOf()[0].classList.contains('table-row')).toBe(true)
  })

  it('gives a field that asked for one a line of its own', () => {
    const withNote = row(3, [
      field('handle', 'h3', { label: 'id', width: 10 }),
      field('title', 'title 3', { label: 'title', width: 50 }),
      field('note', 'worth saying', { label: '' }),
    ])
    render([withNote])
    const note = Array.from(rowsOf()[0].children).find(c => c.textContent.includes('worth saying'))

    expect(note.style.gridColumn).toBe('1 / -1')
    expect(note.classList.contains('table-own-line')).toBe(true)
    // Inside its row, so it keeps the row's box and whatever colour the row is tinted.
    expect(note.parentElement.classList.contains('table-row')).toBe(true)
  })
})

describe('the room a table does not have', () => {
  beforeEach(setupDOM)

  it('drops the padding a field is given to sit in a form', () => {
    // It was about two thirds of the height of a row: a field is padded for a page with half a
    // dozen of them, and a table has a hundred.
    render([withDone(1)])
    for (const cell of rowsOf()[0].children) {
      expect(cell.style.padding, 'a cell kept its form padding').toBe('')
      const value = cell.querySelector('.field-value')
      if (value) expect(value.style.minHeight).toBe('')
    }
  })

  it('leaves a padding it did not choose itself alone', () => {
    // Which is which is read from a flag set where the default is applied, not by recognising
    // the value - there are three possible defaults and they would drift apart from any list
    // kept here. A cell without the flag is the document's business.
    const app = render([withDone(1)])
    const cell = rowsOf()[0].children[0]
    delete cell.dataset.paddingIsDefault
    cell.style.padding = '12px'

    app.renderer.arrangeTables()

    expect(cell.style.padding).toBe('12px')
  })

  it('lets the cells fill the row so a rule between them runs the whole way', () => {
    // A horizontal row centres its children and lets them be as tall as they are, which would
    // leave a separator stopping wherever the shorter cell's text did.
    render([withDone(1)])
    expect(rowsOf()[0].style.alignItems).toBe('')
  })
})

describe('rules between the cells', () => {
  beforeEach(setupDOM)

  it('are off unless asked for', () => {
    render([withDone(1)])
    expect(table().classList.contains('table-lines-columns')).toBe(false)
    expect(table().classList.contains('table-lines-rows')).toBe(false)
  })

  it('can be asked for one at a time', () => {
    render([withDone(1)], { lines: { String: 'vertical' } })
    expect(table().classList.contains('table-lines-columns')).toBe(true)
    expect(table().classList.contains('table-lines-rows'), 'vertical drew horizontals too')
      .toBe(false)

    setupDOM()
    render([withDone(1)], { lines: { String: 'horizontal' } })
    expect(table().classList.contains('table-lines-rows')).toBe(true)
    expect(table().classList.contains('table-lines-columns')).toBe(false)
  })

  it('draws both when simply turned on', () => {
    render([withDone(1)], { lines: { Boolean: true } })
    expect(table().classList.contains('table-lines-columns')).toBe(true)
    expect(table().classList.contains('table-lines-rows')).toBe(true)
  })
})

describe('the heading', () => {
  beforeEach(setupDOM)

  it('names the columns, in order, only when asked for', () => {
    render([withDone(1)])
    expect(document.querySelector('.table-header')).toBeNull()

    setupDOM()
    render([withDone(1)], { header: { Boolean: true } })
    const headings = Array.from(document.querySelectorAll('.table-heading'))
    expect(headings.map(h => h.textContent)).toEqual(['id', 'title', 'done', 'pts'])
    expect(headings.map(h => h.style.gridColumn)).toEqual(['1', '2', '3', '4'])
  })

  it('comes from the template, so an empty list still has one', () => {
    // When it is worth most: a table with nothing in it and no headings is an unexplained blank.
    render([], { header: { Boolean: true } })
    expect(Array.from(document.querySelectorAll('.table-heading')).map(h => h.textContent))
      .toEqual(['id', 'title', 'done', 'pts'])
  })

  it('says nothing about the field that takes its own line', () => {
    // It has no column to head, and a heading over the full width would read as a fifth column.
    render([withDone(1)], { header: { Boolean: true } })
    expect(document.querySelectorAll('.table-heading').length).toBe(4)
  })

  it('sticks only when asked', () => {
    render([withDone(1)], { header: { Boolean: true } })
    expect(document.querySelector('.table-header').classList.contains('sticky')).toBe(false)

    setupDOM()
    render([withDone(1)], { header: { Boolean: true }, sticky: { Boolean: true } })
    expect(document.querySelector('.table-header').classList.contains('sticky')).toBe(true)
  })

  it('is rebuilt rather than added to', () => {
    // The tab bar's bug in a new place: this runs again after every repaint, and appending would
    // leave a table wearing a heading per repaint.
    const app = render([withDone(1)], { header: { Boolean: true } })
    app.renderer.arrangeTables()
    app.renderer.arrangeTables()
    expect(document.querySelectorAll('.table-header').length).toBe(1)
  })
})

describe('a repaint underneath a table', () => {
  beforeEach(setupDOM)

  it('does not knock the cells out of their columns', () => {
    // A field re-rendered after an edit comes back as a fresh element that knows nothing about
    // the grid over it - the same hazard the filter has, handled the same way.
    const app = render([withDone(1), withoutDone(2)])
    expect(placement(rowsOf()[1]).points).toBe('4')

    app.renderer.rerenderSubtree(app.currentDocument, ['t', 'Rows'])

    expect(placement(rowsOf()[1]).points, 'the repainted row left the table').toBe('4')
    expect(table()).toBeTruthy()
  })
})

describe('the stylesheet half', () => {
  const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')

  const bodyOf = (selector) => {
    const pattern = /([^{}]+)\{([^}]*)\}/g
    let match
    while ((match = pattern.exec(styles)) !== null) {
      const [, selectors, body] = match
      if (selectors.split(',').some((one) => one.trim() === selector)) return body
    }
    return ''
  }

  it('makes the list the grid its rows sit in', () => {
    expect(bodyOf('.overseer-list.view-table')).toMatch(/display\s*:\s*grid/)
  })

  it('makes each row a subgrid of it', () => {
    // Rather than `display: contents`, which would align the cells just as well and throw away
    // the row's own box - its background, and so the progress tint on it.
    const body = bodyOf('.overseer-list.view-table > .table-row')
    expect(body).toMatch(/grid-template-columns\s*:\s*subgrid/)
    expect(body).toMatch(/grid-column\s*:\s*1\s*\/\s*-1/)
  })

  it('does not force a display with !important anywhere', () => {
    // What keeps the filter working over a table: the rule that hides a filtered row wins on
    // specificity among `!important` display rules, and a table rule joining that contest would
    // outrank it and put every filtered row back on screen.
    const pattern = /([^{}]+)\{([^}]*)\}/g
    let match
    while ((match = pattern.exec(styles)) !== null) {
      const [, selectors, body] = match
      if (!/display\s*:[^;]*!important/.test(body)) continue
      for (const selector of selectors.split(',')) {
        expect(
          /table-row|table-header|view-table/.test(selector),
          `\`${selector.trim()}\` forces a display with !important, which would outrank the ` +
          `rule that hides a filtered row`
        ).toBe(false)
      }
    }
  })
})
