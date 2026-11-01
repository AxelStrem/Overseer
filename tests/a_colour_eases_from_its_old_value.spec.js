import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// `ease=` eases a node's colours from what was drawn before to what is drawn now. A redraw puts a
// new element in place of the old one, so the values last drawn are remembered by address and the
// new element is animated from them. A first draw eases nothing, nor does a redraw that changed
// nothing, and the theme's `motion = none` turns it off.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const key = (name, value) => base(name, '-', { value: { String: value } })
const theme = (...keys) => ({ ...base('theme', 'context', { _context_copy: { Boolean: true } }, keys) })

/// A cell whose background and easing are formulas the resolver has worked out.
const cell = (colour, ease) => base('cell', 'div', {
  'background-color': { Formula: 'alive ? "#00ff00" : "#000000"' }, '_computed_background-color': { String: colour },
  ease: { Formula: 'alive ? "100ms" : "900ms"' }, _computed_ease: { String: ease },
}, [base('x', 'bool', { value: { Boolean: false } })])

const theDocument = (colour, ease, { themed = [] } = {}) => [
  base('s', 'tab', { mutable: { Boolean: true } }, [...themed, cell(colour, ease)]),
]

const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

let app
let animate
const draw = async (doc) => {
  app.currentDocument = doc
  app.renderer.renderDocument(doc)
  await settled()
}

describe('an easing', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
    app = new OverseerApp()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    animate = vi.fn(() => ({ playState: 'running', cancel() {} }))
    HTMLElement.prototype.animate = animate
  })
  afterEach(() => { delete HTMLElement.prototype.animate })

  it('eases nothing on the first draw', async () => {
    await draw(theDocument('#00ff00', '100ms'))
    expect(animate).not.toHaveBeenCalled()
  })

  it('eases a changed colour from the old value to the new, over its duration', async () => {
    await draw(theDocument('#00ff00', '100ms'))
    await draw(theDocument('#000000', '0.9s'))
    expect(animate).toHaveBeenCalledTimes(1)
    const [[from, to], options] = animate.mock.calls[0]
    expect(from.backgroundColor).toBe('rgb(0, 255, 0)')
    expect(to.backgroundColor).toBe('rgb(0, 0, 0)')
    expect(Object.keys(to)).toEqual(['backgroundColor'])
    expect(options.duration).toBe(900)
  })

  it('eases nothing when nothing it eases has changed', async () => {
    await draw(theDocument('#00ff00', '100ms'))
    await draw(theDocument('#00ff00', '100ms'))
    expect(animate).not.toHaveBeenCalled()
  })

  it('is turned off by the theme', async () => {
    const themed = [theme(key('motion', 'none'))]
    await draw(theDocument('#00ff00', '100ms', { themed }))
    await draw(theDocument('#000000', '900ms', { themed }))
    expect(animate).not.toHaveBeenCalled()
  })
})
