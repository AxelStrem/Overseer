import { describe, it, expect, beforeEach, vi } from 'vitest'

// A list names the columns a reader may sort it by, `sortable="..."`. In a table, the heading of
// each one cycles it through no sort, descending and ascending. The choice is the tab's, kept the
// way a fold is: nothing is sent to the backend, and with no column sorted the list is in the
// order of the document again. While it is sorted, its entries cannot be dragged.

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
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, variant, value, more = {}) => Object.assign(base(name, type), {
  parameters: Object.assign({ value: { [variant]: value } }, more),
})

const vocabularyEntry = (tag) => Object.assign(base(`Label__${tag}`, 'div'), {
  children: [valued('tag', 'string', 'String', tag)],
})

const vocabulary = (name, tags) => Object.assign(base(name, 'list'), {
  parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
  children: tags.map(vocabularyEntry),
})

const COLUMNS = JSON.stringify([
  { name: 'handle', label: 'id', width: '20%', span: false },
  { name: 'stage', label: 'stage', width: '20%', span: false },
  { name: 'flags', label: 'flags', width: '20%', span: false },
  { name: 'points', label: 'pts', width: '20%', span: false },
  { name: 'title', label: 'title', width: '20%', span: false },
])

const item = (handle, stage, flags, points) => Object.assign(base(`Item__${handle}`, 'div'), {
  parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
  children: [
    valued('handle', 'string', 'String', handle),
    valued('stage', 'enum', 'String', stage, { vocabulary: { String: '/project/Stages' } }),
    valued('flags', 'tags', 'String', flags, { vocabulary: { String: '/project/Flags' } }),
    valued('points', 'int', 'Integer', points),
    valued('title', 'string', 'String', `about ${handle}`),
  ],
})

const ITEMS = () => [
  item('a', 'ready', 'first', 3),
  item('b', 'filed', 'asked', 1),
  item('c', 'testing', '', 5),
  item('d', 'filed', 'manual, first', 2),
  item('e', 'planning', 'auto', 8),
]

const docOf = (listParams = {}) => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    vocabulary('Stages', ['filed', 'planning', 'ready', 'testing']),
    vocabulary('Flags', ['manual', 'auto', 'discuss', 'first', 'asked']),
    Object.assign(base('Items', 'list'), {
      parameters: Object.assign({
        view: { String: 'table' },
        header: { Boolean: true },
        _columns: { String: COLUMNS },
        _effective_layout: { String: 'vertical' },
        movable: { Boolean: true },
        sortable: { String: 'handle, stage, flags, points' },
      }, listParams),
      children: ITEMS(),
    }),
  ],
})])

let app
const render = (listParams) => {
  app = new OverseerApp()
  window.app = app
  app.currentFile = 'project.os'
  app.currentDocument = docOf(listParams)
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const table = () => document.querySelector('.overseer-list')
const order = () => Array.from(table().children)
  .filter((r) => !r.classList.contains('table-header'))
  .map((r) => JSON.parse(r.dataset.path).slice(-1)[0].replace('Item__', ''))
const heading = (label) => Array.from(document.querySelectorAll('.table-heading'))
  .find((h) => h.textContent.trim() === label)
const click = (label) => heading(label).dispatchEvent(new MouseEvent('click', { bubbles: true }))

describe('a reader sorts a table by its columns', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockClear()
  })

  it('offers a sort on the columns the list names, and on no other', () => {
    render()
    expect(heading('id').classList.contains('sortable')).toBe(true)
    expect(heading('pts').querySelector('.sort-icon')).not.toBeNull()
    expect(heading('title').classList.contains('sortable')).toBe(false)
    expect(heading('title').querySelector('.sort-icon')).toBeNull()
  })

  it('cycles a column through descending, ascending and back to the document order', () => {
    render()
    expect(order()).toEqual(['a', 'b', 'c', 'd', 'e'])
    click('pts')
    expect(order()).toEqual(['e', 'c', 'a', 'd', 'b'])
    expect(heading('pts').dataset.sort).toBe('desc')
    click('pts')
    expect(order()).toEqual(['b', 'd', 'a', 'c', 'e'])
    expect(heading('pts').dataset.sort).toBe('asc')
    click('pts')
    expect(order()).toEqual(['a', 'b', 'c', 'd', 'e'])
    expect(heading('pts').dataset.sort).toBe('none')
  })

  it('sorts tags by their bits, a later tag in the vocabulary the higher one', () => {
    // asked > first + manual > first > auto; an item with no flags goes last either way.
    render()
    click('flags')
    expect(order()).toEqual(['b', 'd', 'a', 'e', 'c'])
    click('flags')
    expect(order()).toEqual(['e', 'a', 'd', 'b', 'c'])
  })

  it('sorts an enum in the order of its vocabulary, not of its letters', () => {
    render()
    click('stage')
    click('stage')
    expect(order()).toEqual(['b', 'd', 'e', 'a', 'c'])
  })

  it('breaks the ties of the column clicked last with the ones clicked before it', () => {
    render()
    click('pts')
    click('stage')
    click('stage')
    // b and d are both filed: points, descending, decide between them.
    expect(order()).toEqual(['d', 'b', 'e', 'a', 'c'])
  })

  it('keeps the sort for the tab, and sends nothing to the backend', () => {
    render()
    click('pts')
    expect(invoke).not.toHaveBeenCalled()
    // Drawn again from scratch, as a reload would.
    render()
    expect(order()).toEqual(['e', 'c', 'a', 'd', 'b'])
    expect(heading('pts').dataset.sort).toBe('desc')
  })

  it('cannot be dragged while it is sorted, and can again once it is not', () => {
    render()
    expect(table().dataset.movable).toBe('1')
    click('pts')
    expect(table().dataset.movable).toBeUndefined()
    click('pts')
    click('pts')
    expect(table().dataset.movable).toBe('1')
  })

  it('ignores the columns it names outside a table', () => {
    render({ view: { String: 'list' } })
    expect(document.querySelector('.table-heading')).toBeNull()
    expect(app.renderer.sortableColumnsOf(app.currentDocument[0].children[2])).toEqual([])
  })
})
