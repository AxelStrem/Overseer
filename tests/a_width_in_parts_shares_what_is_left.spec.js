import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A width in parts (`2fr`) or `auto` is a share of what is left once the other widths and the gaps
// are taken, so a row or a table fills its container exactly without its widths having to add up
// to 100%. It means the same in a horizontal row, which is flex, as in a table's columns, which are
// a grid. jsdom lays nothing out: what is asserted is what the browser is given.

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
})
const line = (name) => base(name, 'text', { value: { String: name } })
const box = (name, parameters) => base(name, 'div', parameters, [line(name + '_a')])

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const drawn = (path) => document.querySelector(`[data-path='${JSON.stringify(path)}']`)
const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')

describe('a width in parts', () => {
  beforeEach(setupDOM)

  it('is a share of a row, not a CSS width', () => {
    render([base('t', 'tab', {}, [base('row', 'div', { layout: { String: 'horizontal' } }, [
      box('fixed', { width: { CssSize: { Pixels: 200 } } }),
      box('one', { width: { CssSize: { Fraction: 1 } } }),
      box('two', { width: { CssSize: { Fraction: 2.5 } } }),
      box('rest', { width: { CssSize: 'Auto' } }),
    ])])])
    const at = (name) => drawn(['t', 'row', name])
    expect(at('fixed').style.width).toBe('200px')
    expect(at('fixed').classList.contains('takes-parts')).toBe(false)
    expect(at('one').classList.contains('takes-parts')).toBe(true)
    expect(at('one').style.getPropertyValue('--ov-parts')).toBe('1')
    expect(at('two').style.getPropertyValue('--ov-parts')).toBe('2.5')
    expect(at('two').style.width).toBe('')
    expect(at('two').classList.contains('keeps-width')).toBe(false)
    expect(at('rest').classList.contains('takes-rest')).toBe(true)
  })

  it('is read by the row stylesheet', () => {
    expect(styles).toMatch(/\.layout-horizontal > \.takes-parts \{\s*flex: var\(--ov-parts\) 1 0;/)
    expect(styles).toMatch(/\.layout-horizontal > \.takes-rest \{\s*flex: 1 1 auto;/)
  })

  it('reaches a table as a column of the grid, with auto never below its content', () => {
    const columns = JSON.stringify([
      { name: 'name', label: 'Name', width: '2fr', span: false },
      { name: 'note', label: 'Note', width: '1fr', span: false },
      { name: 'count', label: 'Count', width: 'auto', span: false },
      { name: 'size', label: 'Size', width: '80px', span: false },
    ])
    render([base('t', 'tab', {}, [base('Rows', 'list', {
      view: { String: 'table' },
      _columns: { String: columns },
      _effective_layout: { String: 'vertical' },
    }, [base('Row__1', 'div', { layout: { String: 'horizontal' } }, [
      base('name', 'string', { value: { String: 'a' }, width: { CssSize: { Fraction: 2 } } }),
    ])])])])
    expect(document.querySelector('.view-table').style.gridTemplateColumns)
      .toBe('2fr 1fr minmax(auto, 1fr) 80px')
  })

  it('reaches a table with percentages as shares of what the other columns leave', () => {
    // A percentage track is read against the whole width, so widths adding up to 100% beside a
    // column of no width or a fixed one overflowed the list.
    const columns = JSON.stringify([
      { name: 'grip', label: '', width: '', span: false },
      { name: 'name', label: 'Name', width: '70%', span: false },
      { name: 'note', label: 'Note', width: '30%', span: false },
      { name: 'size', label: 'Size', width: '30px', span: false },
    ])
    render([base('t', 'tab', {}, [base('Rows', 'list', {
      view: { String: 'table' },
      _columns: { String: columns },
      _effective_layout: { String: 'vertical' },
    }, [base('Row__1', 'div', { layout: { String: 'horizontal' } }, [
      base('name', 'string', { value: { String: 'a' } }),
    ])])])])
    expect(document.querySelector('.view-table').style.gridTemplateColumns)
      .toBe('auto minmax(0, 0.7fr) minmax(0, 0.3fr) 30px')
  })
})
