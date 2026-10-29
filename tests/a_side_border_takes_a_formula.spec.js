import { describe, it, expect, beforeEach, vi } from 'vitest'

// `border-top` and its siblings take a formula as `border-style` does: a cell of a game can draw
// itself raised until it is opened, `border-top=$(opened ? "solid 1px gray" : "solid 3px silver")`.
// A formula not yet worked out is left alone, never written into the style as its own text.

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

const div = (name, parameters) => ({
  name, node_type: 'div', children: [], is_hierarchy_transparent: false, parameters,
})

const render = (...nodes) => {
  const app = new OverseerApp()
  app.currentDocument = [{ name: 't', node_type: 'tab', parameters: {}, children: nodes, is_hierarchy_transparent: false }]
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const styleOf = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`).style

const raised = { Formula: 'opened ? "solid 1px gray" : "solid 3px silver"' }

describe('a side border', () => {
  beforeEach(() => setupDOM())

  it('is what its formula works out to', () => {
    render(
      div('closed', { 'border-top': raised, '_computed_border-top': { String: 'solid 3px silver' } }),
      div('open', { 'border-bottom': raised, '_computed_border-bottom': { String: 'solid 1px gray' } }),
    )
    expect(styleOf('closed').borderTop).toBe('3px solid silver')
    expect(styleOf('open').borderBottom).toBe('1px solid gray')
  })

  it('takes a formula on each of the four sides', () => {
    const computed = { String: 'solid 2px navy' }
    render(div('cell', {
      'border-top': raised, '_computed_border-top': computed,
      'border-bottom': raised, '_computed_border-bottom': computed,
      'border-left': raised, '_computed_border-left': computed,
      'border-right': raised, '_computed_border-right': computed,
    }))
    const style = styleOf('cell')
    for (const side of [style.borderTop, style.borderBottom, style.borderLeft, style.borderRight]) {
      expect(side).toBe('2px solid navy')
    }
  })

  it('still takes a value written as it is', () => {
    render(div('plain', { 'border-left': { BorderStyle: { Solid: [{ Pixels: 2 }, { Named: 'red' }] } } }))
    expect(styleOf('plain').borderLeft).toBe('2px solid red')
  })

  it('is left alone while its formula has not been worked out', () => {
    render(div('pending', { 'border-top': raised }))
    expect(styleOf('pending').borderTop).toBe('')
  })
})
