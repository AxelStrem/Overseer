import { describe, it, expect, beforeEach, vi } from 'vitest'

// A theme's `backdrop` is the layers painted over its background colour: written as one CSS
// background list, the first layer on top, and painted by a container only where it is not the one
// the container sits on - so a translucent layer is never laid again at every level, and a part
// that starts its backdrop over covers the one around it with its own background.

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

const key = (name, value) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: { value: typeof value === 'number' ? { Integer: value } : { String: value } },
})

const block = (name, children) => ({
  name, node_type: '-', children, is_hierarchy_transparent: false, parameters: {},
})

const gradient = (name, extra = {}) => block(name, [
  key('kind', 'gradient'), key('from', 'red'), key('to', 'blue'),
  ...Object.entries(extra).map(([k, v]) => key(k, v)),
])

const theme = (keys) => ({
  name: 'theme', node_type: 'context', is_hierarchy_transparent: false, children: keys,
  parameters: { _context_copy: { Boolean: true } },
})

const field = (name) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: 'x' } },
})

const div = (name, children) => ({
  name, node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children,
})

const elementAt = (path) => document.querySelector(`[data-path='${JSON.stringify(path)}']`)

describe('a backdrop', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
  })

  it('is its layers in the order written, a gradient blending around its centre', () => {
    const app = new OverseerApp()
    const css = app.renderer.backdropCss(block('backdrop', [
      gradient('glow', { angle: 135, center: '30%', sharpness: '40%' }),
      block('hidden', [key('kind', 'none')]),
      gradient('base'),
    ]))
    expect(css).toBe('linear-gradient(135deg, red 0%, blue 60%), linear-gradient(180deg, red 0%, blue 100%)')
  })

  it('with a sharpness of 100% is a hard edge, and with no layer left is none', () => {
    const app = new OverseerApp()
    expect(app.renderer.backdropCss(block('backdrop', [gradient('edge', { sharpness: '100%' })])))
      .toBe('linear-gradient(180deg, red 50%, blue 50%)')
    expect(app.renderer.backdropCss(block('backdrop', []))).toBe('none')
  })

  it('is painted by a container only where it is not the one around it', () => {
    const app = new OverseerApp()
    const page = [gradient('glow')]
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        div('same', [theme([key('accent', 'green'), block('backdrop', page)]), field('a')]),
        div('own', [theme([block('backdrop', [gradient('glow', { angle: 90 })])]), field('b')]),
        div('bare', [theme([block('backdrop', [])]), field('c')]),
      ] },
      theme([key('background', 'white'), block('backdrop', page)]),
    ]
    app.renderer.renderDocument(app.currentDocument)
    expect(document.documentElement.style.getPropertyValue('--ov-backdrop'))
      .toBe('linear-gradient(180deg, red 0%, blue 100%)')

    const same = elementAt(['t', 'same'])
    expect(same.classList.contains('ov-themed-backdrop')).toBe(false)
    expect(same.classList.contains('ov-themed-background')).toBe(false)

    const own = elementAt(['t', 'own'])
    expect(own.classList.contains('ov-themed-backdrop')).toBe(true)
    expect(own.style.getPropertyValue('--ov-backdrop')).toBe('linear-gradient(90deg, red 0%, blue 100%)')

    // Started over, it paints its background colour and nothing on it, covering the page's.
    const bare = elementAt(['t', 'bare'])
    expect(bare.style.getPropertyValue('--ov-backdrop')).toBe('none')
    expect(bare.classList.contains('ov-themed-backdrop')).toBe(true)
    expect(bare.classList.contains('ov-themed-background')).toBe(true)
  })

  it('of a kind is that kind\'s own', () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [field('a')] },
      theme([block('button', [block('backdrop', [gradient('shine')])])]),
    ]
    app.renderer.renderDocument(app.currentDocument)
    const root = document.documentElement
    expect(root.style.getPropertyValue('--ov-button-backdrop')).toBe('linear-gradient(180deg, red 0%, blue 100%)')
    expect(root.style.getPropertyValue('--ov-backdrop')).toBe('')
  })
})
