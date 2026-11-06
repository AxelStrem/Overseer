import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// `hover-text` on a button says what it does, as on a field: a sentence outright, `true` its label,
// and on a button drawn as an icon alone the name it goes by. A button answers `context hover-text`
// as a field does; a div and a chart say only their own, since a div answering an inherited `true`
// would cover everything inside it with its name.

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

/** Chart.js needs a canvas context jsdom does not provide. */
vi.mock('chart.js', () => {
  class Chart {
    destroy() {}
    update() {}
    static register() {}
  }
  const stub = {}
  return {
    Chart,
    CategoryScale: stub, LinearScale: stub, PointElement: stub, LineElement: stub,
    LineController: stub, ArcElement: stub, PieController: stub, DoughnutController: stub,
    BarElement: stub, BarController: stub, Title: stub, Tooltip: stub, Legend: stub,
  }
})
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const said = (value) => (typeof value === 'boolean' ? { Boolean: value } : { String: value })

const button = (name, { label, icon, saying } = {}) => {
  const parameters = {}
  if (label !== undefined) parameters.label = { String: label }
  if (icon !== undefined) parameters.icon = { String: icon }
  if (saying !== undefined) parameters['hover-text'] = said(saying)
  return base(name, 'button', parameters)
}

const field = (name, label) => base(name, 'string', { value: { String: 'x' }, label: { String: label } })

const contextTrue = () => base('hover-text', 'context', { value: { Boolean: true }, _context_copy: { Boolean: true } })

const render = (...children) => {
  const app = new OverseerApp()
  app.currentDocument = [base('t', 'tab', {}, children)]
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const at = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', ...path])}']`)
const buttonAt = (...path) => {
  const found = at(...path)
  return found?.tagName === 'BUTTON' ? found : found?.querySelector('button')
}

describe('a button asked what it does', () => {
  beforeEach(() => {
    setupDOM()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
    document.querySelectorAll('.overseer-hover-text').forEach((n) => n.remove())
  })

  it('says the sentence it was given, in the popup and not in the browser title as well', () => {
    render(button('rem', { icon: 'trash-2', saying: 'Remove this record' }))
    const rem = buttonAt('rem')
    expect(rem.dataset.hoverText).toBe('Remove this record')
    expect(rem.hasAttribute('title')).toBe(false)
    expect(rem.getAttribute('aria-label')).toBe('rem')

    rem.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
    vi.advanceTimersByTime(400)
    expect(document.querySelector('.overseer-hover-text').textContent).toBe('Remove this record')
  })

  it('says its label when asked with true', () => {
    render(button('go', { label: 'Finished', saying: true }))
    expect(buttonAt('go').dataset.hoverText).toBe('Finished')
  })

  it('says its name when drawn as an icon alone, and its icon where it has no name', () => {
    render(button('note', { icon: 'pencil', saying: true }), button('', { icon: 'pin', saying: true }))
    expect(buttonAt('note').dataset.hoverText).toBe('note')
    expect(document.querySelector('button.overseer-button[aria-label="pin"]')?.dataset.hoverText).toBe('pin')
  })

  it('keeps the browser title where nothing is said', () => {
    render(button('note', { icon: 'pencil' }))
    const note = buttonAt('note')
    expect(note.dataset.hoverText).toBeUndefined()
    expect(note.title).toBe('note')
  })

  it('answers a context hover-text as a field does, under a sentence of its own', () => {
    render(base('Row', 'div', {}, [
      contextTrue(),
      button('drop', { icon: 'cross' }),
      button('pin', { icon: 'pin', saying: 'Keep it on top of the list' }),
    ]))
    expect(buttonAt('Row', 'drop').dataset.hoverText).toBe('drop')
    expect(buttonAt('Row', 'pin').dataset.hoverText).toBe('Keep it on top of the list')
  })
})

describe('a div or a chart asked what it is', () => {
  beforeEach(() => setupDOM())
  afterEach(() => vi.unstubAllGlobals())

  it('a div says its own sentence, and a field inside with its own still says that', () => {
    render(base('Card', 'div', { 'hover-text': said('Press to record it as done') }, [
      base('weight', 'string', { value: { String: '1' }, label: { String: 'Weight' }, 'hover-text': said('In kg') }),
    ]))
    const card = document.querySelector('.overseer-div[data-name="Card"]')
    expect(card.dataset.hoverText).toBe('Press to record it as done')
    expect(at('Card', 'weight').closest('[data-hover-text]').dataset.hoverText).toBe('In kg')
  })

  it('a div does not answer the context it declares for what is inside it', () => {
    render(base('Row', 'div', { label: { String: 'Row' } }, [contextTrue(), field('title', 'Title')]))
    const row = document.querySelector('.overseer-div[data-name="Row"]')
    expect(row.dataset.hoverText).toBeUndefined()
    expect(at('Row', 'title').closest('[data-hover-text]').dataset.hoverText).toBe('Title')
  })

  it('a chart says its own sentence', () => {
    // jsdom has no ResizeObserver, which a chart watches its size with.
    vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} })
    render(base('trend', 'chart', { 'hover-text': said('Blood pressure, by week') }, [
      base('sys', 'plot', { label: { String: 'sys' }, _computed_series: { String: JSON.stringify([[1, 2], [2, 5]]) } }),
    ]))
    expect(document.querySelector('.overseer-chart').dataset.hoverText).toBe('Blood pressure, by week')
  })
})
