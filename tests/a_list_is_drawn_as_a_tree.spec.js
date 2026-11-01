import { describe, it, expect, beforeEach, vi } from 'vitest'

// A list that says `tree=` is drawn as an outline: each entry under its parent, indented by its
// depth, with lines joining them and a chevron that folds a branch away. The hierarchy is either
// the entries' own lists of children (`tree="Parts"`) or a field naming each entry's parent
// (`tree="parent"`), and both are drawn as one run of rows inside the list. The filter keeps a
// match's ancestors in sight, dimmed, and hides the rest.

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

// Side by side: `[handle, parent]`, in the order written.
const task = ([handle, parent], n) => base(`Task__${n + 1}`, 'div', {}, [
  text('handle', handle), text('title', `Title of ${handle}`), text('parent', parent),
])
const flat = (tasks) => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('find', 'filter', { target: { String: '/t/Tasks' }, text: { String: 'title' } }),
  base('Tasks', 'list', { key: { String: 'handle' }, tree: { String: 'parent' } }, tasks.map(task)),
])]

// Nested: each part holds its own list of parts.
const part = (name, n, kids = []) => base(`Part__${n}`, 'div', {}, [
  text('name', name),
  base('Parts', 'list', { key: { String: 'name' } }, kids.map(([k, sub], i) => part(k, i + 1, sub))),
])
const nested = () => [base('o', 'tab', {}, [
  base('Parts', 'list', { key: { String: 'name' }, tree: { String: 'Parts' } }, [
    part('Car', 1, [['Engine', [['Pistons', []]]], ['Wheels', []]]),
    part('Bicycle', 2),
  ]),
])]

let app
const open = (doc) => {
  app = new OverseerApp()
  app.currentDocument = doc
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const list = () => document.querySelector('.overseer-list.view-tree')
const rows = () => Array.from(list().querySelectorAll(':scope > .tree-row'))
const nameOf = (row) => {
  const field = Array.from(row.querySelectorAll('[data-path]'))
    .find((el) => /"(handle|name)"\]$/.test(el.dataset.path))
  return (field?.textContent || '').trim()
}
const outline = () => rows()
  .filter((row) => !row.classList.contains('tree-folded-away') && !row.classList.contains('filtered-out'))
  .map((row) => `${'  '.repeat(Number(row.dataset.treeDepth))}${nameOf(row)}`)
const rowNamed = (name) => rows().find((row) => nameOf(row) === name)
const lines = (name) => Array.from(rowNamed(name).querySelectorAll(':scope > .tree-lines > *'))
  .map((piece) => `${piece.className}@${piece.style.getPropertyValue('--tree-column')}`)

describe('a list drawn as a tree', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
  })

  it('puts each entry under the one it names as its parent, siblings in the order written', () => {
    open(flat([['site', ''], ['pages', 'site'], ['about', 'pages'], ['deploy', 'site'], ['home', 'pages'], ['launch', '']]))
    expect(outline()).toEqual(['site', '  pages', '    about', '    home', '  deploy', 'launch'])
  })

  it('draws an entry whose parent is missing, or which is in a cycle, at the top', () => {
    open(flat([['x', 'y'], ['y', 'x'], ['lost', 'gone'], ['z', 'y']]))
    expect(outline()).toEqual(['x', '  y', '    z', 'lost'])
  })

  it('draws a nested tree as one run of rows, each at its own address', () => {
    open(nested())
    expect(outline()).toEqual(['Car', '  Engine', '    Pistons', '  Wheels', 'Bicycle'])
    expect(rowNamed('Pistons').dataset.path)
      .toBe(JSON.stringify(['o', 'Parts', 'Part__1', 'Parts', 'Part__1', 'Parts', 'Part__1']))
    expect(list().querySelector('.overseer-list'), 'an inner list was drawn as a box').toBeNull()
  })

  it('joins each row to its parent, and runs a guide on only past a sibling still to come', () => {
    open(nested())
    expect(lines('Car')).toEqual(['tree-stem@0', 'tree-chevron@0'])
    expect(lines('Engine')).toEqual(['tree-elbow@0', 'tree-branch@0', 'tree-stem@1', 'tree-chevron@1'])
    expect(lines('Pistons')).toEqual(['tree-guide@0', 'tree-elbow tree-elbow-last@1', 'tree-branch tree-branch-leaf@1'])
    expect(lines('Wheels')).toEqual(['tree-elbow tree-elbow-last@0', 'tree-branch tree-branch-leaf@0'])
    expect(lines('Bicycle')).toEqual([])
  })

  it('folds a branch away and back from its chevron, in the page alone, and keeps the fold', () => {
    open(flat([['site', ''], ['pages', 'site'], ['about', 'pages'], ['deploy', 'site']]))
    const chevron = () => rowNamed('pages').querySelector('.tree-chevron')
    chevron().dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(outline()).toEqual(['site', '  pages', '  deploy'])
    expect(chevron().getAttribute('aria-expanded')).toBe('false')
    expect(invoke.mock.calls.length, 'a fold asked the backend').toBe(0)

    app.renderer.renderDocument(app.currentDocument)
    expect(outline(), 'the fold was lost in a redraw').toEqual(['site', '  pages', '  deploy'])

    chevron().dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(outline()).toEqual(['site', '  pages', '    about', '  deploy'])
  })

  it('keeps a match in sight with its ancestors dimmed, and hides the rest', () => {
    open(flat([['site', ''], ['pages', 'site'], ['about', 'pages'], ['home', 'pages'], ['deploy', 'site']]))
    const input = document.querySelector('.overseer-filter input')
    input.value = 'about'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    app.renderer.applyFilters()
    expect(outline()).toEqual(['site', '  pages', '    about'])
    expect(rowNamed('site').classList.contains('tree-ancestor')).toBe(true)
    expect(rowNamed('about').classList.contains('tree-ancestor')).toBe(false)
    // The last child that shows ends the line, whatever is hidden after it.
    expect(lines('about')).toContain('tree-elbow tree-elbow-last@1')
    expect(lines('pages')).toContain('tree-elbow tree-elbow-last@0')
  })

  it('moves an entry when the parent it names changes', () => {
    open(flat([['site', ''], ['pages', 'site'], ['launch', '']]))
    const field = app.currentDocument[0].children[1].children[2].children[2]
    field.parameters.value = { String: 'pages' }
    app.renderer.arrangeTrees()
    expect(outline()).toEqual(['site', '  pages', '    launch'])
  })
})
