import { describe, it, expect, beforeEach, vi } from 'vitest'

// A list that is both `view="table"` and `tree=` keeps its columns aligned: the rows stay as wide
// as the table, and only the cell of one column - the key's, which is what a parent names, or
// the first where the key is no column - stands in by its depth, with the tree's lines over it.
// Where the theme draws no lines, or says `stand = "row"`, the whole row stands in instead, and
// any lines are drawn in the gutter its depth leaves on its left.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})
const text = (name, value) => base(name, 'string', { value: { String: value } })

const COLUMNS = JSON.stringify([
  { name: 'title', label: 'title', width: '60%', span: false },
  { name: 'handle', label: 'id', width: '20%', span: false },
  { name: 'parent', label: 'of', width: '20%', span: false },
])

const task = ([handle, parent], n) => base(`Task__${n + 1}`, 'div',
  { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
  [text('title', `Title of ${handle}`), text('handle', handle), text('parent', parent)])

const doc = (tasks, key = 'handle') => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Tasks', 'list', {
    key: { String: key },
    tree: { String: 'parent' },
    view: { String: 'table' },
    header: { Boolean: true },
    _columns: { String: COLUMNS },
    _effective_layout: { String: 'vertical' },
  }, tasks.map(task)),
])]

let app
const open = (d) => {
  app = new OverseerApp()
  app.currentDocument = d
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const list = () => document.querySelector('.overseer-list.view-tree.view-table')
const rows = () => Array.from(list().querySelectorAll(':scope > .tree-row'))
const field = (row, name) => row.querySelector(`:scope > [data-path$='"${name}"]']`)

describe('a tree drawn inside a table', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
  })

  it('puts its rows in tree order, each a row of the table', () => {
    open(doc([['site', ''], ['launch', ''], ['pages', 'site'], ['about', 'pages']]))
    expect(rows().map((row) => `${row.dataset.treeDepth}:${field(row, 'handle').textContent.trim()}`))
      .toEqual(['0:site', '1:pages', '2:about', '0:launch'])
    for (const row of rows()) expect(row.classList.contains('table-row')).toBe(true)
  })

  it('stands in the key column only, and lays the lines over it rather than in a column of their own', () => {
    open(doc([['site', ''], ['pages', 'site']]))
    expect(list().dataset.treeAt).toBe('2')
    for (const row of rows()) {
      expect(field(row, 'handle').classList.contains('tree-cell')).toBe(true)
      expect(field(row, 'title').classList.contains('tree-cell')).toBe(false)
      expect(field(row, 'title').style.gridColumn).toBe('1')
      const lines = row.querySelector(':scope > .tree-lines')
      expect(lines.style.gridColumn).toBe('2')
      expect(lines.classList.contains('table-cell')).toBe(false)
    }
  })

  it('draws the lines again in their column after the table is arranged again', () => {
    open(doc([['site', ''], ['pages', 'site']]))
    app.renderer.arrangeTrees()
    app.renderer.arrangeTables()
    for (const row of rows()) {
      expect(row.querySelectorAll(':scope > .tree-lines').length).toBe(1)
      expect(row.querySelector(':scope > .tree-lines').style.gridColumn).toBe('2')
    }
  })

  it('stands in the first column where the key is no column', () => {
    open(doc([['site', ''], ['pages', 'site']], 'missing'))
    expect(list().dataset.treeAt).toBe('1')
    expect(field(rows()[1], 'title').classList.contains('tree-cell')).toBe(true)
  })

  it('draws no lines where the theme says none, only the chevrons, and stands the rows in whole', () => {
    open(doc([['site', ''], ['pages', 'site'], ['about', 'pages']]))
    list().style.setProperty('--ov-tree-style', 'none')
    app.renderer.drawTreeLines(list())
    expect(list().classList.contains('tree-unlined')).toBe(true)
    expect(list().querySelectorAll('.tree-guide, .tree-elbow, .tree-branch, .tree-stem')).toHaveLength(0)
    expect(list().querySelectorAll('.tree-chevron')).toHaveLength(2)
    // The cells before the tree's column move with the row; the first of them is on its edge.
    for (const row of rows()) {
      expect(field(row, 'title').classList.contains('tree-before')).toBe(true)
      expect(field(row, 'title').classList.contains('tree-edge')).toBe(true)
      expect(field(row, 'parent').classList.contains('tree-before')).toBe(false)
    }
  })

  it('draws its lines again once the theme says how', () => {
    open(doc([['site', ''], ['pages', 'site']]))
    list().style.setProperty('--ov-tree-style', 'none')
    app.renderer.drawTreeLines(list())
    list().style.setProperty('--ov-tree-style', 'dashed')
    app.renderer.drawTreeLines(list())
    expect(list().classList.contains('tree-unlined')).toBe(false)
    expect(list().querySelectorAll('.tree-elbow')).toHaveLength(1)
  })

  it('stands the rows in whole and draws their lines in the gutter on their left where the theme says row', () => {
    open(doc([['site', ''], ['pages', 'site'], ['about', 'pages'], ['blog', 'site']]))
    list().style.setProperty('--ov-tree-stand', 'row')
    app.renderer.drawTreeLines(list())
    expect(list().classList.contains('tree-stands')).toBe(true)
    expect(list().classList.contains('tree-unlined')).toBe(false)
    const [site, pages, about, blog] = rows()
    // A top-level row has no gutter; each row under it draws its own line in, and the guide of an
    // ancestor whose siblings are still to come.
    expect(site.querySelector(':scope > .tree-gutter')).toBeNull()
    const kinds = (row) => Array.from(row.querySelectorAll(':scope > .tree-gutter > span')).map((s) => s.className)
    expect(kinds(pages)).toEqual(['tree-elbow', 'tree-branch'])
    expect(kinds(about)).toEqual(['tree-guide', 'tree-elbow tree-elbow-last', 'tree-branch tree-branch-leaf'])
    expect(kinds(blog)).toEqual(['tree-elbow tree-elbow-last', 'tree-branch tree-branch-leaf'])
    // The chevrons stay in the tree's column, and nothing is drawn there under a row's own start.
    for (const row of rows()) {
      expect(row.querySelectorAll(':scope > .tree-lines > span')).toHaveLength(0)
    }
    expect(list().querySelectorAll('.tree-lines > .tree-chevron')).toHaveLength(2)
  })

  it("keeps the gutter out of the table's columns when the table is arranged again", () => {
    open(doc([['site', ''], ['pages', 'site']]))
    list().style.setProperty('--ov-tree-stand', 'row')
    app.renderer.drawTreeLines(list())
    app.renderer.arrangeTables()
    const gutter = rows()[1].querySelector(':scope > .tree-gutter')
    expect(gutter).not.toBeNull()
    expect(gutter.style.gridColumn).toBe('')
  })

  it('stands only the column with its lines where the theme says column, even without lines', () => {
    open(doc([['site', ''], ['pages', 'site']]))
    list().style.setProperty('--ov-tree-style', 'none')
    list().style.setProperty('--ov-tree-stand', 'column')
    app.renderer.drawTreeLines(list())
    expect(list().classList.contains('tree-stands')).toBe(false)
  })
})
