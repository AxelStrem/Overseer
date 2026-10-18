import { describe, it, expect, beforeEach, vi } from 'vitest'

// `shown` gives the text a field is drawn as in place of its value: a count column that is mostly
// zeros reads best with the zeros left blank. It is the whole text - no precision, format, prefix
// or suffix is added to it - and it is only drawn: the editor opens on the value itself.

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

const field = (name, node_type, value, more = {}) => ({
  name, node_type, children: [], is_hierarchy_transparent: false,
  parameters: Object.assign({ value }, more),
})

const counted = (name, n, more = {}) => field(name, 'int', { Integer: n }, Object.assign({
  shown: { Formula: `${name} == 0 ? "" : ${name}` },
  _computed_shown: n === 0 ? { String: '' } : { Integer: n },
}, more))

const render = (...fields) => {
  const app = new OverseerApp()
  window.app = app
  app.currentDocument = [{
    name: 't', node_type: 'tab', parameters: { mutable: { Boolean: true } }, children: fields,
    is_hierarchy_transparent: false,
  }]
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const valueOf = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}'] .field-value`)

describe('a field drawn as its shown text', () => {
  beforeEach(() => setupDOM())

  it('draws a zero as nothing and any other count as itself', () => {
    render(counted('main', 1), counted('perf', 0))
    expect(valueOf('main').textContent).toBe('1')
    expect(valueOf('perf').textContent).toBe('')
  })

  it('is the whole text, with no precision or suffix added', () => {
    render(
      field('share', 'float', { Float: 1.5 }, {
        precision: { Integer: 2 }, suffix: { String: ' %' },
        shown: { Formula: 'share == 0 ? "-" : share' }, _computed_shown: { Float: 1.5 },
      }),
      field('nothing', 'float', { Float: 0 }, {
        suffix: { String: ' kg' }, shown: { Formula: 'nothing == 0 ? "" : nothing' }, _computed_shown: { String: '' },
      }),
      field('plain', 'float', { Float: 1.5 }, { precision: { Integer: 2 }, suffix: { String: ' %' } }),
    )
    expect(valueOf('share').textContent).toBe('1.5')
    expect(valueOf('nothing').textContent).toBe('')
    expect(valueOf('plain').textContent).toBe('1.50 %')
  })

  it('takes a stated text as readily as a formula', () => {
    render(field('weight', 'float', { Float: 82.46 }, { suffix: { String: ' kg' }, shown: { String: 'unweighed' } }))
    expect(valueOf('weight').textContent).toBe('unweighed')
  })

  it('draws a formula that gives nothing as nothing', () => {
    render(counted('main', 0, { _computed_shown: 'Null' }))
    expect(valueOf('main').textContent).toBe('')
  })

  it('goes on strings and dates as on numbers', () => {
    render(
      field('note', 'string', { String: '' }, { shown: { Formula: 'note == "" ? "?" : note' }, _computed_shown: { String: '?' } }),
      field('due', 'date', { Date: '2026-10-19' }, { shown: { String: 'soon' } }),
    )
    expect(valueOf('note').textContent).toBe('?')
    expect(valueOf('due').textContent).toBe('soon')
  })

  it('draws an enum as its text in place of the chip', () => {
    render(field('kind', 'enum', { String: 'audio' }, { shown: { String: 'a' } }))
    const chip = document.querySelector(`[data-path='${JSON.stringify(['t', 'kind'])}'] .tag-chip`)
    expect(chip.classList.contains('enum-shown')).toBe(true)
    expect(chip.textContent).toBe('a')
  })

  it('opens the editor on the value, not on what is drawn', () => {
    render(counted('perf', 0))
    valueOf('perf').dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    expect(document.querySelector('input.field-editor').value).toBe('0')
  })

  it('is drawn again from the new node when the value changes', () => {
    const app = render(counted('perf', 0))
    const element = document.querySelector(`[data-path='${JSON.stringify(['t', 'perf'])}']`)
    app.renderer.updateSingleElement(element, counted('perf', 3), ['t', 'perf'])
    expect(valueOf('perf').textContent).toBe('3')
    app.renderer.updateSingleElement(element, counted('perf', 0), ['t', 'perf'])
    expect(valueOf('perf').textContent).toBe('')
  })
})
