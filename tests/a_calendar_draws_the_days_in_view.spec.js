import { describe, it, expect, beforeEach, vi } from 'vitest'

// A calendar holds a cell for every day it wrote and every day in view, which the resolver gives
// in date order, each named by its date and marked whether it is in view. The page draws the days
// in view one to a row, each headed by its date unless the calendar says `header=false`. A day
// out of view is kept, and not drawn.

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

const day = (name, value, inView) => Object.assign(base(name, 'int'), {
  parameters: { value: { Integer: value }, _in_view: { Boolean: inView } },
})

const calendarOf = ({ header } = {}) => {
  const parameters = { entry: { String: 'int' }, from: { String: '2026-11-03' }, to: { String: '2026-11-05' } }
  if (header !== undefined) parameters.header = { Boolean: header }
  const children = [
    day('2026-10-20', 2, false),
    day('2026-11-03', 0, true),
    day('2026-11-04', 5, true),
    day('2026-11-05', 0, true),
  ]
  return [Object.assign(base('t', 'tab'), {
    children: [Object.assign(base('Days', 'calendar'), { parameters, children })],
  })]
}

const calendar = () => document.querySelector('.overseer-grid')
const drawnDays = () => Array.from(calendar().children)
  .filter(el => el.dataset && el.dataset.path)
  .map(el => JSON.parse(el.dataset.path).slice(-1)[0])

describe('a calendar', () => {
  const render = (doc) => {
    const app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('draws the days in view and not a day kept outside them', () => {
    render(calendarOf())
    expect(drawnDays()).toEqual(['2026-11-03', '2026-11-04', '2026-11-05'])
  })

  it('heads each day with its date, to its left', () => {
    render(calendarOf())
    const order = Array.from(calendar().children).map(el =>
      el.classList.contains('overseer-grid-heading') ? `#${el.textContent}` : JSON.parse(el.dataset.path).slice(-1)[0])
    expect(order).toEqual(['#2026-11-03', '2026-11-03', '#2026-11-04', '2026-11-04', '#2026-11-05', '2026-11-05'])
    expect(calendar().style.gridTemplateColumns).toBe('auto minmax(0, 1fr)')
  })

  it('draws no dates when it says header=false', () => {
    render(calendarOf({ header: false }))
    expect(calendar().querySelectorAll('.overseer-grid-heading')).toHaveLength(0)
    expect(calendar().style.gridTemplateColumns).toBe('minmax(0, 1fr)')
  })
})
