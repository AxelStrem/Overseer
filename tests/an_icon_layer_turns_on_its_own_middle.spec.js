import { describe, it, expect, beforeEach, vi } from 'vitest'

// An icon layer that turns turns on its own middle, wherever its `position` puts it, rather than
// about the middle of the box it is drawn over - which, on a wide row, would swing it across the
// row. A gradient still turns about the box's middle.


function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        </div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <div id="tab-container"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="loading-screen" class="screen"><div id="loading-display" class="page-display"></div></div>
      <div id="welcome-screen" class="screen"></div>
      <div id="editor-screen" class="screen"><div id="content-display"></div></div>
      <div id="error-screen" class="screen"><div id="error-display" class="page-display"></div>
        <p id="error-message"></p><button id="error-back-btn"></button></div>
    </div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { OverseerApp } from '../src/main.js'

const SVG = (mark) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="${mark}"/></svg>`

const key = (name, value) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: { value: typeof value === 'number' ? { Integer: value } : { String: value } },
})

const block = (name, children) => ({
  name, node_type: '-', children, is_hierarchy_transparent: false, parameters: {},
})

const icon = (name, extra = {}) => block(name, [
  key('kind', 'icon'),
  ...Object.entries(extra).map(([k, v]) => key(k, v)),
])

const theme = (keys) => ({
  name: 'theme', node_type: 'context', is_hierarchy_transparent: false, children: keys,
  parameters: { _context_copy: { Boolean: true } },
})

const div = (name, children) => ({
  name, node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children,
})

const field = (name) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: 'x' } },
})

const holderOf = (element) => Array.from(element.children).find((c) => c.classList.contains('ov-backdrop-motion'))
const settle = () => new Promise((resolve) => queueMicrotask(resolve))

/// Asks for the layers until nothing is on its way: the library's names, then each icon.
async function layersOf(renderer, backdrop) {
  for (let i = 0; i < 5; i++) {
    const got = renderer.backdropLayers(backdrop)
    if (!got.waiting.length) return got
    await Promise.all(got.waiting)
  }
  throw new Error('still waiting')
}

import { iconTurnOrigin } from '../src/renderer.js'

describe('an icon layer that turns', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
    globalThis.fetch = vi.fn(async (url) => {
      if (url === '/icons/index.json') return { ok: true, json: async () => ['loader-circle'] }
      const name = /^\/icons\/(.+)\.svg$/.exec(url)?.[1]
      return name ? { ok: true, text: async () => SVG(name) } : { ok: false, text: async () => '' }
    })
  })

  it('finds the middle of an icon wherever CSS would place it', () => {
    expect(iconTurnOrigin('center', '20px')).toBe('50% 50%')
    expect(iconTurnOrigin('left 14px top 6px', '26px')).toBe('calc(14px + 26px / 2) calc(6px + 26px / 2)')
    expect(iconTurnOrigin('right 10px bottom 4px', '20px')).toBe('calc(100% - 10px - 20px / 2) calc(100% - 4px - 20px / 2)')
    // A share of a background's place is one of the room the icon leaves, and a share of the
    // box's height is the holder's.
    expect(iconTurnOrigin('25% center', '50%')).toBe('calc(((100% - 50cqh) * 0.25) + 50cqh / 2) 50%')
    expect(iconTurnOrigin('left calc(14% - 24px) center', '20px'))
      .toBe('calc((((100% - 20px) * 0.14) - 24px) + 20px / 2) 50%')
    expect(iconTurnOrigin('top right', '20px')).toBe('calc(100% - ((100% - 20px) * 0) - 20px / 2) calc(((100% - 20px) * 0) + 20px / 2)')
    expect(iconTurnOrigin('left left', '20px')).toBeNull()
  })

  it('turns its box about the icon, and a gradient about the box', async () => {
    const app = new OverseerApp()
    const backdrop = block('backdrop', [
      icon('busy', { source: 'loader-circle', size: '20px', position: 'left 100px center', motion: 'turn', period: '3s' }),
      block('glow', [key('kind', 'gradient'), key('from', 'red'), key('to', 'blue'), key('motion', 'turn')]),
    ])
    await layersOf(app.renderer, backdrop)
    app.renderer.applyDocumentContexts([theme([backdrop])])
    await settle()
    const [glow, busy] = holderOf(document.body).children
    expect(busy.dataset.layer).toBe('busy')
    expect(busy.className).toBe('ov-backdrop-layer ov-backdrop-turn')
    expect(busy.style.transformOrigin).toBe('calc(100px + 20px / 2) 50%')
    expect(busy.style.getPropertyValue('--ov-backdrop-period')).toBe('3s')
    expect(glow.style.transformOrigin).toBe('')
  })

  it('takes a place written with calc, but nothing that could reach another layer', async () => {
    const app = new OverseerApp()
    const place = async (position) => {
      const { layers } = await layersOf(app.renderer, block('backdrop', [icon('busy', { source: 'loader-circle', position })]))
      return layers[0].position
    }
    expect(await place('left calc(14% - 24px) center')).toBe('left calc(14% - 24px) center')
    expect(await place('left, url(x.png)')).toBe('center')
    expect(await place('url(x.png)')).toBe('center')
  })
})
