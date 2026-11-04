import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A field's `alignment` in a table: its value sits in the middle of its cell, or at the end, and
// the column's heading goes with it, so a centred chip has its name centred over it.

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

const field = (name, value, label, alignment) => {
  const node = Object.assign(base(name, 'string'), {
    parameters: { value: { String: String(value) }, label: { String: label } },
  })
  if (alignment) node.parameters.alignment = { String: alignment }
  return node
}

/** As the resolver hands it over, the field's `alignment` carried on its column. */
const COLUMNS = JSON.stringify([
  { name: 'title', label: 'title', width: '60%', span: false, align: '' },
  { name: 'stage', label: 'stage', width: '20%', span: false, align: 'center' },
  { name: 'points', label: 'pts', width: '20%', span: false, align: 'far' },
])

const render = () => {
  const row = Object.assign(base('Row__1', 'div'), {
    parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
    children: [field('title', 'a title', 'title'), field('stage', 'ready', 'stage', 'center'),
      field('points', '3', 'pts', 'far')],
  })
  const doc = [Object.assign(base('t', 'tab'), {
    children: [Object.assign(base('Rows', 'list'), {
      parameters: {
        view: { String: 'table' }, header: { Boolean: true },
        _columns: { String: COLUMNS }, _effective_layout: { String: 'vertical' },
      },
      children: [row],
    })],
  })]
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(doc)
}

const cell = (name) => document.querySelector(`.table-row > [data-path$='"${name}"]']`)
const heading = (label) => Array.from(document.querySelectorAll('.table-heading'))
  .find((h) => h.textContent === label)

describe('a field can be centred in its column', () => {
  beforeEach(setupDOM)

  it('places the cell as its field says', () => {
    render()
    expect(cell('stage').classList.contains('column-center')).toBe(true)
    expect(cell('points').classList.contains('column-far')).toBe(true)
    expect(cell('title').classList.contains('column-center')).toBe(false)
    expect(cell('title').classList.contains('column-far')).toBe(false)
  })

  it('takes the heading with it', () => {
    render()
    expect(heading('stage').classList.contains('column-center')).toBe(true)
    expect(heading('pts').classList.contains('column-far')).toBe(true)
    expect(heading('title').className).toBe('table-heading')
  })

  it('is what the stylesheet moves', () => {
    const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
    expect(styles).toMatch(/\.overseer-field\.column-center\s*\{\s*justify-content:\s*center/)
    expect(styles).toMatch(/\.table-heading\.column-center\s*\{\s*text-align:\s*center/)
  })
})
