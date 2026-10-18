import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A theme says sizes, spacing and type as well as colours: `size`, `leading`, `numerals`, and keys
// of each kind - `table { row { rule = 1px } }`, `chip { remove = pointed }`. A key in words is
// written beside itself as the CSS values it means, a table hands on what it says of the kinds in
// it, and a button is marked for its style. jsdom does not work out `var()`, so the look itself is
// seen in the real page; this holds the parts together.

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

const base = (name, type) => ({ name, node_type: type, parameters: {}, is_hierarchy_transparent: false, children: [] })
const key = (name, value) => Object.assign(base(name, '-'), { parameters: { value: { String: value } } })
const group = (name, ...children) => Object.assign(base(name, '-'), { children })
const theme = (...children) => Object.assign(base('theme', 'context'), {
  children, parameters: { _context_copy: { Boolean: true } },
})
const div = (name, children) => Object.assign(base(name, 'div'), { children })
const button = (name, parameters = {}) => Object.assign(base(name, 'button'), {
  parameters: { label: { String: name }, ...parameters },
})

const aTable = (...more) => {
  const columns = JSON.stringify([
    { name: 'title', label: 'title', width: '50%', span: false },
    { name: 'go', label: 'go', width: '20%', span: false },
  ])
  const row = (n) => Object.assign(base(`Row__${n}`, 'div'), {
    parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
    children: [
      Object.assign(base('title', 'string'), { parameters: { value: { String: `row ${n}` } } }),
      button('go'),
    ],
  })
  return Object.assign(base('Rows', 'list'), {
    parameters: {
      view: { String: 'table' },
      header: { Boolean: true },
      _columns: { String: columns },
      _effective_layout: { String: 'vertical' },
    },
    children: [row(1), row(2), ...more],
  })
}

const settled = () => new Promise((resolve) => setTimeout(resolve, 0))
const buttonAt = (path) => document.querySelector(`button[data-path='${JSON.stringify(path)}']`)
const containerAt = (path) => document.querySelector(`[data-path='${JSON.stringify(path)}']`)

describe('a theme that says sizes, spacing and type', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
  })

  it('writes a key said in words as the CSS it means, and today\'s look as unsaid', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        div('plain', [theme(group('table', group('heading', key('case', 'as-written')), key('frame', 'box')))]),
      ] },
      theme(
        key('numerals', 'tabular'),
        group('table',
          key('frame', 'none'),
          group('heading', key('case', 'upper'), key('tracking', '0.05'), key('rule', 'none')),
          group('row', key('rule', '1px'), key('pointed', 'tint')),
          key('cell-padding', '8px'),
          key('detail', 'bar')),
        group('field', key('value-frame', 'none')),
        group('chip', key('remove', 'pointed'), key('add', 'never')),
      ),
    ]
    app.renderer.renderDocument(app.currentDocument)
    await settled()

    const root = document.documentElement.style
    const said = (name) => root.getPropertyValue(name)
    expect(said('--ov-numerals--css')).toBe('tabular-nums')
    expect(said('--ov-table-frame--border')).toBe('none')
    expect(said('--ov-table-heading-case--css')).toBe('uppercase')
    expect(said('--ov-table-heading-tracking--css')).toBe('0.05em')
    expect(said('--ov-table-heading-rule--css')).toBe('0px')
    expect(said('--ov-table-row-rule--css')).toBe('1px')
    expect(said('--ov-table-row-rule--gap')).toBe('0px')
    expect(said('--ov-table-row-pointed--tint')).toMatch(/gradient/)
    expect(said('--ov-table-cell-padding--start')).toBe('0px')
    expect(said('--ov-table-detail--side')).toMatch(/solid/)
    expect(said('--ov-field-value-frame--pad')).toBe('0px')
    expect(said('--ov-chip-remove--rest')).toBe('none')
    expect(said('--ov-chip-add--pointed')).toBe('hidden')

    // A part that says today's look undoes what the theme around it said.
    const plain = containerAt(['t', 'plain']).style
    expect(plain.getPropertyValue('--ov-table-heading-case--css')).toBe('none')
    expect(plain.getPropertyValue('--ov-table-frame--border')).toBe('initial')
  })

  it('draws the buttons in a table as the table says, and the others as their own key says', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        button('outside'),
        aTable(),
      ] },
      theme(
        group('button', key('style', 'plain')),
        group('table', group('button', key('style', 'outline'), key('height', '22px'))),
      ),
    ]
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
    await settled()

    const table = document.querySelector('.overseer-list.view-table')
    expect(table, 'the table did not render').not.toBeNull()
    expect(table.style.getPropertyValue('--ov-button-height')).toBe('22px')
    expect(table.style.getPropertyValue('--ov-button-style')).toBe('outline')

    const inside = buttonAt(['t', 'Rows', 'Row__1', 'go'])
    expect(inside.classList.contains('ov-style-outline')).toBe(true)
    expect(inside.classList.contains('ov-style-plain')).toBe(false)
    expect(buttonAt(['t', 'outside']).classList.contains('ov-style-plain')).toBe(true)
  })

  it('leaves a button the document coloured itself in its own colours', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        button('own', { 'background-color': { String: '#16a34a' } }),
        button('themed'),
      ] },
      theme(group('button', key('style', 'outline'))),
    ]
    app.renderer.renderDocument(app.currentDocument)
    await settled()

    expect(buttonAt(['t', 'own']).classList.contains('ov-style-outline')).toBe(false)
    expect(buttonAt(['t', 'themed']).classList.contains('ov-style-outline')).toBe(true)
  })

  it('draws nothing differently where the theme says none of it', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [aTable()] },
      theme(key('accent', '#2563eb')),
    ]
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
    await settled()

    const table = document.querySelector('.overseer-list.view-table')
    expect(Array.from(table.style).filter((name) => name.startsWith('--ov-'))).toEqual([])
    const go = buttonAt(['t', 'Rows', 'Row__1', 'go'])
    expect(Array.from(go.classList).filter((c) => c.startsWith('ov-style-'))).toEqual([])
  })

  it('has the stylesheet read every key, with today\'s value where none is said', () => {
    const styles = readFileSync('src/styles.css', 'utf8')
    const read = [
      ['--ov-size'], ['--ov-leading', '1.6'], ['--ov-numerals--css'],
      ['--ov-field-value-frame--pad', '6px 8px'], ['--ov-field-spacing', '8px'], ['--ov-field-number-align', 'right'],
      ['--ov-table-frame--border', '1px solid var(--ov-line)'], ['--ov-table-heading-size', '11px'],
      ['--ov-table-heading-weight', '600'], ['--ov-table-heading-case--css', 'none'],
      ['--ov-table-heading-tracking--css', '0.03em'], ['--ov-table-heading-rule--css', '1px'],
      ['--ov-table-row-padding', '8px'], ['--ov-table-row-rule--css', '1px'], ['--ov-table-row-pointed--tint', 'none'],
      ['--ov-table-cell-padding', '6px'], ['--ov-table-detail--side'], ['--ov-table-detail-padding'],
      ['--ov-chip-size', '11px'], ['--ov-chip-padding', '1px 7px'], ['--ov-chip-label'],
      ['--ov-chip-remove--rest', 'inline-block'], ['--ov-chip-add--rest', 'visible'],
      ['--ov-button-height', 'auto'], ['--ov-button-padding', '8px 16px'], ['--ov-button-size', '14px'],
      ['--ov-button-weight', '500'], ['--ov-button-spacing', '8px'],
      ['--ov-tab-size', '14px'], ['--ov-tab-weight', 'normal'], ['--ov-tab-padding', '8px 16px'],
      ['--ov-filter-width', 'auto'], ['--ov-filter-size', '12px'], ['--ov-filter-padding', '3px 8px'],
    ]
    for (const [name, today] of read) {
      const at = styles.indexOf(`var(${name}${today ? `, ${today}` : ''}`)
      expect(at, `nothing reads ${name}${today ? ` falling back to ${today}` : ''}`).toBeGreaterThan(-1)
    }
    for (const way of ['outline', 'plain']) {
      expect(styles, `nothing draws ${way}`).toMatch(new RegExp(`\\.overseer-button\\.ov-style-${way}`))
    }
  })
})
