import { describe, it, expect, beforeEach, vi } from 'vitest'

// A layer of a theme's backdrop can move on its own - `motion` turn, drift or breathe - in the page
// alone. Where one does, every layer of that backdrop is drawn in a box of its own inside a holder
// behind what the element shows, and the element paints no backdrop itself; the boxes are kept
// across a redraw, so that a press does not start them over.

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
const holderOf = (element) => Array.from(element.children).find((c) => c.classList.contains('ov-backdrop-motion'))
const settle = () => new Promise((resolve) => queueMicrotask(resolve))

describe('a backdrop layer that moves', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
  })

  it('says its motion and period, and a still or unknown one says none', () => {
    const app = new OverseerApp()
    const { layers } = app.renderer.backdropLayers(block('backdrop', [
      gradient('glow', { motion: 'turn', period: '30s', angle: 90 }),
      gradient('wave', { motion: 'drift', period: '800ms' }),
      gradient('soft', { motion: 'breathe', period: 'soon' }),
      gradient('odd', { motion: 'spin' }),
      gradient('base'),
    ]))
    expect(layers.map((l) => [l.name, l.motion, l.period])).toEqual([
      ['glow', 'turn', 30], ['wave', 'drift', 0.8], ['soft', 'breathe', 8], ['odd', null, 0], ['base', null, 0],
    ])
    expect(layers[0].gradient.angle).toBe(90)
  })

  it('draws the page\'s backdrop in boxes behind it, the first layer on top, and keeps them when drawn again', async () => {
    const app = new OverseerApp()
    const doc = (to) => [theme([block('backdrop', [
      gradient('glow', { motion: 'turn', period: 40 }),
      block('base', [key('kind', 'gradient'), key('from', 'black'), key('to', to)]),
    ])])]
    app.renderer.applyDocumentContexts(doc('white'))
    await settle()
    const body = document.body
    expect(body.classList.contains('ov-backdrop-moving')).toBe(true)
    const holder = holderOf(body)
    expect(holder.getAttribute('aria-hidden')).toBe('true')
    expect(body.lastElementChild).toBe(holder)
    const [base, glow] = holder.children
    expect(base.dataset.layer).toBe('base')
    expect(base.className).toBe('ov-backdrop-layer')
    expect(base.style.backgroundImage).toBe('linear-gradient(180deg, black 0%, white 100%)')
    expect(glow.dataset.layer).toBe('glow')
    expect(glow.className).toBe('ov-backdrop-layer ov-backdrop-square ov-backdrop-turn')
    expect(glow.style.getPropertyValue('--ov-backdrop-period')).toBe('40s')

    // A change of colour, as a tick makes, keeps the same boxes, so the turn goes on.
    app.renderer.applyDocumentContexts(doc('grey'))
    await settle()
    expect(holderOf(body)).toBe(holder)
    expect(holder.children[0]).toBe(base)
    expect(holder.children[1]).toBe(glow)
    expect(base.style.backgroundImage).toBe('linear-gradient(180deg, black 0%, grey 100%)')

    // Stilled, the holder goes and the body paints the backdrop again.
    app.renderer.applyDocumentContexts([theme([block('backdrop', [gradient('glow', { motion: 'none' })])])])
    expect(holderOf(body)).toBeUndefined()
    expect(body.classList.contains('ov-backdrop-moving')).toBe(false)
  })

  it('drifts a gradient along its angle in a larger square, its blend drawn shorter to look the same', () => {
    const app = new OverseerApp()
    app.renderer.applyDocumentContexts([theme([block('backdrop', [gradient('glow', { motion: 'drift', angle: 90 })])])])
    const box = holderOf(document.body).children[0]
    expect(box.className).toBe('ov-backdrop-layer ov-backdrop-square ov-backdrop-drift')
    expect(box.style.getPropertyValue('--ov-backdrop-scale')).toBe('1.5')
    expect(box.style.backgroundImage).toBe('linear-gradient(90deg, red 16.67%, blue 83.33%)')
    expect(box.style.getPropertyValue('--ov-backdrop-drift-x')).toBe('16.67%')
    expect(box.style.getPropertyValue('--ov-backdrop-drift-y')).toBe('0%')
  })

  it('stands still where the theme says motion is none', () => {
    const app = new OverseerApp()
    app.renderer.applyDocumentContexts([theme([key('motion', 'none'), block('backdrop', [gradient('glow', { motion: 'turn' })])])])
    expect(holderOf(document.body)).toBeUndefined()
    expect(document.documentElement.style.getPropertyValue('--ov-backdrop')).toBe('linear-gradient(180deg, red 0%, blue 100%)')
  })

  it('in a container of its own is drawn after the container\'s children, and only where it is its own', async () => {
    const app = new OverseerApp()
    const page = [gradient('glow', { motion: 'breathe' })]
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        div('own', [theme([block('backdrop', [gradient('glow', { motion: 'breathe', angle: 90 })])]), field('a')]),
        div('same', [theme([key('accent', 'green'), block('backdrop', page)]), field('b')]),
      ] },
      theme([block('backdrop', page)]),
    ]
    app.renderer.renderDocument(app.currentDocument)
    await settle()
    const own = elementAt(['t', 'own'])
    const holder = holderOf(own)
    expect(own.lastElementChild).toBe(holder)
    expect(own.firstElementChild).not.toBe(holder)
    expect(own.classList.contains('ov-backdrop-moving')).toBe(true)
    expect(own.style.position).toBe('relative')
    expect(holder.children[0].className).toBe('ov-backdrop-layer ov-backdrop-breathe')
    expect(holderOf(elementAt(['t', 'same']))).toBeUndefined()
  })
})
