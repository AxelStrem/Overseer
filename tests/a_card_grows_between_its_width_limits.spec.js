import { describe, it, expect, beforeEach, vi } from 'vitest'

// `min-width` and `max-width` bound how wide a node is drawn, so a card of `width=fit-content`
// grows with what it holds up to a limit. On a `flow` container `min-width` already meant how
// narrow its columns may get, and keeps that meaning: it is no size of the container's own.

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
  name, node_type: 'div', children: [line(name + '_a')], is_hierarchy_transparent: false, parameters,
})

const render = (...nodes) => {
  const app = new OverseerApp()
  app.currentDocument = [{
    name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: nodes,
  }]
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const drawn = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)

describe('width limits', () => {
  beforeEach(() => setupDOM())

  it('bound a card that fits its content', () => {
    render(box('card', {
      width: { CssSize: 'FitContent' },
      'min-width': { CssSize: { Pixels: 260 } },
      'max-width': { CssSize: { Pixels: 420 } },
    }))
    const card = drawn('card')
    expect(card.style.width).toBe('fit-content')
    expect(card.style.minWidth).toBe('260px')
    expect(card.style.maxWidth).toBe('420px')
  })

  it('take a bare number as pixels', () => {
    render(box('card', { 'max-width': { Integer: 300 } }))
    expect(drawn('card').style.maxWidth).toBe('300px')
  })

  it('leave a node that names none unbounded', () => {
    render(box('card', {}))
    expect(drawn('card').style.minWidth).toBe('')
    expect(drawn('card').style.maxWidth).toBe('')
  })

  it('keep min-width on a flow container as the narrowest column, not its own size', () => {
    render(box('grid', { layout: { String: 'flow' }, 'min-width': { CssSize: { Pixels: 260 } } }))
    const grid = drawn('grid')
    expect(grid.style.minWidth).toBe('')
    expect(grid.style.gridTemplateColumns).toBe('repeat(auto-fit, minmax(260px, 1fr))')
  })
})
