import { describe, it, expect, beforeEach, vi } from 'vitest'

// A field takes the padding it states.
//
// A parameter arrives as the value it was parsed into - `{ CssSize: { Pixels: 4 } }` for `4px`,
// `{ Integer: 0 }` for `0` - and the layout code took it for a number or a string, so it wrote the
// style `[object Object]`, which the browser throws away. Stating a padding still counted as
// stating one, and suppressed the default: a field that asked for 4px or 12px got none at all,
// and 0 looked right only by accident. Margins went the same way.

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

const field = (name, parameters) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false,
  parameters: Object.assign({ value: { String: name } }, parameters),
})

const render = (...children) => {
  const app = new OverseerApp()
  app.currentDocument = [{ name: 't', node_type: 'tab', parameters: {}, children, is_hierarchy_transparent: false }]
  app.renderer.renderDocument(app.currentDocument)
}

const box = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)

describe('a stated padding', () => {
  beforeEach(() => setupDOM())

  it('is applied in pixels, as stated', () => {
    render(
      field('four', { padding: { CssSize: { Pixels: 4 } } }),
      field('twelve', { padding: { CssSize: { Pixels: 12 } } }),
    )
    expect(box('four').style.padding).toBe('4px')
    expect(box('twelve').style.padding).toBe('12px')
  })

  it('is applied when it is a plain number, nought included', () => {
    render(
      field('none', { padding: { Integer: 0 } }),
      field('six', { padding: { Integer: 6 } }),
    )
    expect(box('none').style.padding).toBe('0px')
    expect(box('six').style.padding).toBe('6px')
  })

  it('is applied when it is written out in full', () => {
    render(field('sides', { padding: { String: '4px 10px' } }))
    expect(box('sides').style.padding).toBe('4px 10px')
  })

  it('is applied on one side', () => {
    render(field('top', { 'padding-top': { CssSize: { Pixels: 9 } } }))
    expect(box('top').style.paddingTop).toBe('9px')
  })

  it('takes what a formula worked out', () => {
    render(field('worked', { padding: { Formula: '../gap' }, _computed_padding: { Integer: 3 } }))
    expect(box('worked').style.padding).toBe('3px')
  })

  it('leaves the default where nothing is stated', () => {
    render(field('plain', {}))
    expect(box('plain').style.padding).toBe('8px')
  })
})

describe('a stated margin', () => {
  beforeEach(() => setupDOM())

  it('is applied in pixels too', () => {
    render(field('spaced', { margin: { CssSize: { Pixels: 12 } } }), field('flush', { margin: { Integer: 0 } }))
    expect(box('spaced').style.margin).toBe('12px')
    expect(box('flush').style.margin).toBe('0px')
  })
})
