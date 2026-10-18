import { describe, it, expect, beforeEach, vi } from 'vitest'

// A size written without a unit - `width=150`, `font-size=14` - is in pixels. It arrives as
// `{ Integer: 150 }`, or as the bare number once a parameter is read through its value.

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

const field = (name, parameters = {}) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false,
  parameters: Object.assign({ value: { String: name } }, parameters),
})

const div = (name, parameters, children = []) => ({
  name, node_type: 'div', children, is_hierarchy_transparent: false, parameters,
})

const render = (...nodes) => {
  const app = new OverseerApp()
  app.currentDocument = [{ name: 't', node_type: 'tab', parameters: {}, children: nodes, is_hierarchy_transparent: false }]
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const at = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', ...path])}']`)

describe('a bare size', () => {
  beforeEach(() => setupDOM())

  it('sizes a node in pixels, and keeps its width in a row', () => {
    render(div('box', { width: { Integer: 150 }, height: { Integer: 0 } }))
    expect(at('box').style.width).toBe('150px')
    expect(at('box').style.height).toBe('0px')
    expect(at('box').classList.contains('keeps-width')).toBe(true)
  })

  it('sets a font size in pixels, on the field and on its value', () => {
    render(field('big', { 'font-size': { Integer: 14 } }))
    expect(at('big').style.fontSize).toBe('14px')
    expect(at('big').querySelector('.field-value').style.fontSize).toBe('14px')
  })

  it('sets a margin in pixels', () => {
    const app = render()
    const element = document.createElement('div')
    app.renderer.applyMarginStyles(element, div('m', { 'margin-top': { Integer: 8 }, margin: { Float: 2.5 } }))
    expect(element.style.marginTop).toBe('8px')
    expect(element.style.marginLeft).toBe('2.5px')
  })

  it('gives a flow list its columns at least that wide', () => {
    const app = render()
    const element = document.createElement('div')
    app.renderer.applyFlowColumns(element, div('l', { 'min-width': { Integer: 150 } }), 'flow')
    expect(element.style.gridTemplateColumns).toBe('repeat(auto-fit, minmax(150px, 1fr))')
  })
})
