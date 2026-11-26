import { describe, it, expect, beforeEach, vi } from 'vitest'

// An icon layer keeps its shape, as wide as it is high, unless it says how wide with `width`:
// then it is stretched to that, as a project tracker stretches an armchair into the bench its idle
// agents sit on.


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
import { iconTurnOrigin } from '../src/renderer.js'

const SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M3 16h18"/></svg>'

const key = (name, value) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value } },
})

const block = (name, children) => ({
  name, node_type: '-', children, is_hierarchy_transparent: false, parameters: {},
})

const icon = (extra = {}) => block('backdrop', [block('seat', [
  key('kind', 'icon'), key('source', 'armchair'),
  ...Object.entries(extra).map(([k, v]) => key(k, v)),
])])

const svgOf = (css) => new DOMParser()
  .parseFromString(decodeURIComponent(/data:image\/svg\+xml,([^"]*)/.exec(css)[1]), 'image/svg+xml').documentElement

async function layersOf(renderer, backdrop) {
  for (let i = 0; i < 5; i++) {
    const got = renderer.backdropLayers(backdrop)
    if (!got.waiting.length) return got
    await Promise.all(got.waiting)
  }
  throw new Error('still waiting')
}

describe('an icon layer stretched', () => {
  beforeEach(() => {
    setupDOM()
    globalThis.fetch = vi.fn(async (url) => ({ ok: true, text: async () => SVG }))
  })

  it('keeps its shape where it says no width', async () => {
    const { renderer } = new OverseerApp()
    const got = await layersOf(renderer, icon({ size: '120%' }))
    expect(got.size).toBe('auto 120%')
    expect(svgOf(got.image).hasAttribute('preserveAspectRatio')).toBe(false)
  })

  it('is stretched to its width, a length or a share of the box, its strokes one width', async () => {
    const { renderer } = new OverseerApp()
    for (const [width, css] of [['64px', '64px'], ['30%', '30%'], ['40', '40%']]) {
      const got = await layersOf(renderer, icon({ size: '120%', width }))
      expect(got.size).toBe(`${css} 120%`)
      expect(svgOf(got.image).getAttribute('preserveAspectRatio')).toBe('none')
      expect(svgOf(got.image).querySelector('path').getAttribute('vector-effect')).toBe('non-scaling-stroke')
    }
  })

  it('keeps its shape for a width that is no single length', async () => {
    const { renderer } = new OverseerApp()
    for (const width of ['1px, url(x)', 'wide', '64px 10px']) {
      const got = await layersOf(renderer, icon({ width }))
      expect(got.size).toBe('auto 100%')
      expect(svgOf(got.image).hasAttribute('preserveAspectRatio')).toBe(false)
    }
  })

  it('turns on its own middle, as wide as it is stretched', () => {
    expect(iconTurnOrigin('right 10px bottom 4px', '20px', '60px'))
      .toBe('calc(100% - 10px - 60px / 2) calc(100% - 4px - 20px / 2)')
    expect(iconTurnOrigin('25% center', '50%', '30%')).toBe('calc(((100% - 30cqw) * 0.25) + 30cqw / 2) 50%')
  })
})
