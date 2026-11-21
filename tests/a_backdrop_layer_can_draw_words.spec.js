import { describe, it, expect, beforeEach, vi } from 'vitest'

// A `text` layer of a theme's backdrop draws words over the box, on one line, as an icon layer
// draws an icon: in its colour, at its opacity, its size high and at its position. A project
// tracker's item draws the name of the agent at work on it this way.

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
  parameters: { value: typeof value === 'number' ? { Float: value } : { String: value } },
})

const block = (name, children) => ({
  name, node_type: '-', children, is_hierarchy_transparent: false, parameters: {},
})

const words = (name, extra = {}) => block(name, [
  key('kind', 'text'),
  ...Object.entries(extra).map(([k, v]) => key(k, v)),
])

const svgOf = (css) => decodeURIComponent(/data:image\/svg\+xml,([^"]*)/.exec(css)[1])

describe('a backdrop layer of words', () => {
  beforeEach(setupDOM)

  it('paints its words in its colour, at its opacity, size and position', () => {
    const { renderer } = new OverseerApp()
    const got = renderer.backdropLayers(block('backdrop', [
      words('worker', { text: 'claude-opus-5-5 (medium)', colour: '#22c55e', opacity: 0.5, size: '14px',
        position: 'right 8px top 1px', weight: 'bold' }),
    ]))
    expect(got.size).toBe('auto 14px')
    expect(got.position).toBe('right 8px top 1px')
    expect(got.repeat).toBe('no-repeat')
    const svg = new DOMParser().parseFromString(svgOf(got.image), 'image/svg+xml').documentElement
    expect(svg.getAttribute('opacity')).toBe('0.5')
    const text = svg.querySelector('text')
    expect(text.textContent).toBe('claude-opus-5-5 (medium)')
    expect(text.getAttribute('fill')).toBe('#22c55e')
    expect(text.getAttribute('font-weight')).toBe('bold')
  })

  it('keeps markup in its words as words', () => {
    const { renderer } = new OverseerApp()
    const got = renderer.backdropLayers(block('backdrop', [words('stamp', { text: '<b>"x" & \'y\')</b>' })]))
    const svg = new DOMParser().parseFromString(svgOf(got.image), 'image/svg+xml').documentElement
    expect(svg.querySelectorAll('*')).toHaveLength(1)
    expect(svg.querySelector('text').textContent).toBe('<b>"x" & \'y\')</b>')
  })

  it('is as high as the letters of its box where it says no size, in the text colour', () => {
    const { renderer } = new OverseerApp()
    const got = renderer.backdropLayers(block('backdrop', [words('stamp', { text: 'DRAFT' })]), '#123456')
    expect(got.size).toBe('auto 1em')
    expect(got.position).toBe('center')
    expect(svgOf(got.image)).toContain('fill="#123456"')
  })

  it('paints nothing for no words, a colour the page does not take, or a sparkle', () => {
    const { renderer } = new OverseerApp()
    for (const layer of [
      words('empty', { text: '  ' }),
      words('unsaid'),
      words('bad', { text: 'x', colour: 'red;background:url(x)' }),
      words('scattered', { text: 'x', motion: 'sparkle' }),
    ]) {
      expect(renderer.backdropLayers(block('backdrop', [layer])).image).toBe('none')
    }
  })

  it('stands beside the other layers in the order written', () => {
    const { renderer } = new OverseerApp()
    const got = renderer.backdropLayers(block('backdrop', [
      words('stamp', { text: 'DRAFT' }),
      block('base', [key('kind', 'gradient'), key('from', 'red'), key('to', 'blue')]),
    ]))
    expect(got.layers.map((l) => l.name)).toEqual(['stamp', 'base'])
    expect(got.size).toBe('auto 1em, auto')
  })
})
