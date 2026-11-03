import { describe, it, expect, beforeEach, vi } from 'vitest'

// A list kept in parts reads only its newest parts. How many entries the older ones hold is not
// known without reading them, so the note says how many parts it left, and still offers one window
// more.

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

const inView = (n) => Object.assign(base(`Day__${n}`, 'div'), {
  parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
  children: [
    Object.assign(base('date', 'string'), {
      parameters: { value: { String: `2026-09-${String(n).padStart(2, '0')}` }, label: { String: '' } },
    }),
  ],
})

const outOfView = (n) => Object.assign(base(`Day__${n}`, '-'), {
  parameters: { _out_of_view: { Boolean: true } },
  children: [
    Object.assign(base('date', 'string'), {
      parameters: { value: { String: `2026-08-${String(n).padStart(2, '0')}` } },
    }),
  ],
})

const docOf = (children, left, parts) => ([Object.assign(base('t', 'tab'), {
  children: [
    Object.assign(base('Days', 'list'), {
      parameters: {
        _effective_layout: { String: 'vertical' },
        window: { Integer: 3 },
        ...(left ? { _left_out_of_view: { Integer: left } } : {}),
        ...(parts ? { _parts_left_unread: { Integer: parts } } : {}),
      },
      children,
    }),
  ],
})])

const render = (doc) => {
  const app = new OverseerApp()
  app.currentFile = 'p.os'
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const said = () => document.querySelector('.overseer-entries-left-out-count')?.textContent
const ask = () => document.querySelector('.overseer-entries-left-out .overseer-show-more')
const dates = () => Array.from(document.querySelectorAll('.field-value'))
  .map(e => e.textContent)
  .filter(v => typeof v === 'string' && v.startsWith('20'))

describe('a list with parts it has not read says so', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
  })

  it('says how many parts it left, with nothing out of view', () => {
    render(docOf([inView(1), inView(2), inView(3)], 0, 2))
    expect(said()).toBe('2 older parts not shown')
    expect(ask().textContent).toBe('show 3 more')
  })

  it('says both, and offers a whole window', () => {
    render(docOf([inView(1), inView(2), inView(3), outOfView(4)], 1, 1))
    expect(said()).toBe('1 earlier entry not shown, and 1 older part')
    expect(ask().textContent).toBe('show 3 more')
  })

  it('says nothing once every part is read', () => {
    render(docOf([inView(1), inView(2), inView(3)], 0, 0))
    expect(said()).toBeUndefined()
  })
})
