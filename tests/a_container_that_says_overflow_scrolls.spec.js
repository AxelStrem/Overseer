import { describe, it, expect, beforeEach, vi } from 'vitest'

// A container can say it scrolls. The server sends the overflow wrapped - `auto` as a CSS size,
// `hidden` as a string - and the page once assigned it as it came, which the browser ignores, so
// nothing ever scrolled. A container that scrolls is marked, so that in a row it takes the row's
// height rather than adding its own.

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

const line = (name) => ({
  name, node_type: 'text', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: name } },
})

const box = (name, parameters) => ({
  name, node_type: 'div', children: [line(name + '_a'), line(name + '_b')],
  is_hierarchy_transparent: false, parameters,
})

const render = (...nodes) => {
  const app = new OverseerApp()
  app.currentDocument = [{ name: 't', node_type: 'tab', parameters: {}, children: nodes, is_hierarchy_transparent: false }]
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const drawn = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)

describe('a container’s overflow', () => {
  beforeEach(() => setupDOM())

  it('reaches the page as the server sends it', () => {
    render(
      box('down', { height: { CssSize: { Pixels: 150 } }, 'overflow-y': { CssSize: 'Auto' } }),
      box('across', { 'overflow-x': { String: 'hidden' } }),
      box('both', { overflow: { String: 'scroll' } }),
    )
    expect(drawn('down').style.overflowY).toBe('auto')
    expect(drawn('across').style.overflowX).toBe('hidden')
    expect(drawn('both').style.overflow).toBe('scroll')
  })

  it('is left alone when it names nothing CSS knows', () => {
    render(box('odd', { 'overflow-y': { String: 'sideways' } }))
    expect(drawn('odd').style.overflowY).toBe('')
    expect(drawn('odd').classList.contains('scrolls')).toBe(false)
  })

  it('marks a container that keeps its contents within its height, and only that one', () => {
    render(
      box('down', { 'overflow-y': { CssSize: 'Auto' }, width: { CssSize: { Percentage: 50 } } }),
      box('free', { overflow: { String: 'hidden' } }),
      box('across', { 'overflow-x': { String: 'auto' } }),
      box('row', { height: { CssSize: { Pixels: 200 } } }),
    )
    expect(drawn('down').classList.contains('scrolls')).toBe(true)
    expect(drawn('down').classList.contains('scrolls-free-width')).toBe(false)
    expect(drawn('free').classList.contains('scrolls-free-width')).toBe(true)
    expect(drawn('across').classList.contains('scrolls')).toBe(false)
    expect(drawn('row').classList.contains('states-height')).toBe(true)
  })
})

describe('a size with no number', () => {
  beforeEach(() => setupDOM())

  it('is auto or fit-content, as stated', () => {
    render(
      box('auto', { height: { CssSize: 'Auto' } }),
      box('fit', { width: { CssSize: 'FitContent' } }),
    )
    expect(drawn('auto').style.height).toBe('auto')
    expect(drawn('fit').style.width).toBe('fit-content')
  })
})
