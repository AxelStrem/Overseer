import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// `mark (type="separator")` is a line. It runs across the way its parent lays things out - down a
// row, along a column - unless its own `layout` says which way, and the theme's `separator` key
// says how it is drawn. In a table each one has a column of its own, so the same separator lines
// up from row to row and on through the heading.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const field = (name, value) => base(name, 'string', { value: { String: String(value) } })

/** As the resolver hands one over: unnamed, transparent, and `_label_layout` where it runs along. */
const separator = (params = {}) => Object.assign(base('mark', 'mark', Object.assign({ type: { String: 'separator' } }, params)), {
  is_hierarchy_transparent: true,
})

const render = (children) => {
  const app = new OverseerApp()
  app.currentDocument = [base('t', 'tab', {}, children)]
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const separators = () => Array.from(document.querySelectorAll('.mark-separator'))

describe('a separator', () => {
  beforeEach(setupDOM)

  it('runs down a row and along a column', () => {
    render([
      base('across', 'div', { _effective_layout: { String: 'horizontal' } }, [field('a', 1), separator(), field('b', 2)]),
      base('down', 'div', { _effective_layout: { String: 'vertical' } }, [
        field('c', 3), separator({ _label_layout: { String: 'horizontal' } }), field('d', 4),
      ]),
    ])
    const [inRow, inColumn] = separators()
    expect(inRow.classList.contains('separator-vertical')).toBe(true)
    expect(inColumn.classList.contains('separator-horizontal')).toBe(true)
  })

  it('runs the way its own layout says', () => {
    render([
      base('across', 'div', { _effective_layout: { String: 'horizontal' } }, [
        field('a', 1), separator({ layout: { String: 'horizontal' } }), field('b', 2),
      ]),
    ])
    expect(separators()[0].classList.contains('separator-horizontal')).toBe(true)
  })

  it('is drawn as the theme says, with the theme line colour where it says none', () => {
    const styles = readFileSync('src/styles.css', 'utf8')
    const rule = styles.match(/\.mark-separator\.separator-vertical::before\s*\{([^}]*)\}/)
    expect(rule, 'no rule draws the line').toBeTruthy()
    expect(rule[1]).toMatch(/var\(--ov-separator-style/)
    expect(rule[1]).toMatch(/var\(--ov-separator-thickness/)
    expect(rule[1]).toMatch(/var\(--ov-separator-color,\s*var\(--ov-line\)\)/)
    expect(rule[1]).toMatch(/height\s*:\s*var\(--ov-separator-length\)/)
  })
})

describe('separators in a table', () => {
  beforeEach(setupDOM)

  const COLUMNS = JSON.stringify([
    { name: 'mark#0', label: '', width: '', span: false, separator: false },
    { name: 'handle', label: 'id', width: '10%', span: false, separator: false },
    { name: 'mark#1', label: '', width: '0px', span: false, separator: true },
    { name: 'title', label: 'title', width: '50%', span: false, separator: false },
    { name: 'mark#2', label: '', width: '0px', span: false, separator: true },
    { name: 'points', label: 'pts', width: '10%', span: false, separator: false },
    { name: 'mark#3', label: '', width: '', span: true, separator: true },
  ])

  const row = (n, { title = true } = {}) => base(`Row__${n}`, 'div', {
    layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' },
  }, [
    Object.assign(base('mark', 'mark', { type: { String: 'grip' } }), { is_hierarchy_transparent: true }),
    field('handle', `h${n}`),
    separator(),
    ...(title ? [field('title', `title ${n}`)] : []),
    separator(),
    field('points', n),
    separator({ layout: { String: 'horizontal' } }),
  ])

  const table = (rows) => {
    render([base('Rows', 'list', {
      view: { String: 'table' }, header: { Boolean: true },
      _columns: { String: COLUMNS }, _effective_layout: { String: 'vertical' },
    }, rows)])
    return document.querySelector('.view-table')
  }

  it('puts each in its own column on every row, the heading included', () => {
    const list = table([row(1), row(2, { title: false })])
    const placed = (r) => Array.from(r.querySelectorAll(':scope > .mark-separator.separator-vertical'))
      .map((s) => s.style.gridColumn)
    const rows = Array.from(list.querySelectorAll(':scope > .table-row'))
    expect(placed(rows[0])).toEqual(['3', '5'])
    // A row without a title leaves its column empty; the separators on either side stay put.
    expect(placed(rows[1])).toEqual(['3', '5'])
    expect(placed(list.querySelector(':scope > .table-header'))).toEqual(['3', '5'])
    expect(list.style.gridTemplateColumns).toBe('1fr 10% 0px 50% 0px 10%')
  })

  it('rules a line of its own across the row when it runs along it', () => {
    const list = table([row(1)])
    const along = list.querySelector('.table-row > .separator-horizontal')
    expect(along.classList.contains('table-own-line')).toBe(true)
    expect(along.style.gridColumn).toBe('1 / -1')
  })

  it('leaves the grip in its own column', () => {
    const list = table([row(1)])
    expect(list.querySelector('.table-row > .mark-grip').style.gridColumn).toBe('1')
  })
})
