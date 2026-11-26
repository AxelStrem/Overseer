import { describe, it, expect, beforeEach, vi } from 'vitest'

// A gradient layer fills the box it is painted on, unless it says how large it is with `size`:
// then it is a box of that size at its `position`, and stays so however the box grows - the corner
// of a project tracker's asked item, which kept growing as the item unfolded.


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

const key = (name, value) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value } },
})

const block = (name, children) => ({
  name, node_type: '-', children, is_hierarchy_transparent: false, parameters: {},
})

const gradient = (extra = {}) => block('backdrop', [block('corner', [
  key('kind', 'gradient'), key('from', '#f59e0b'), key('to', 'transparent'), key('angle', '225'),
  ...Object.entries(extra).map(([k, v]) => key(k, v)),
])])

describe('a gradient layer', () => {
  beforeEach(setupDOM)

  it('fills the whole box where it says no size', () => {
    const { renderer } = new OverseerApp()
    const got = renderer.backdropLayers(gradient())
    expect(got.size).toBe('auto')
    expect(got.position).toBe('0% 0%')
    expect(got.repeat).toBe('repeat')
  })

  it('is a box of its size at its position, once', () => {
    const { renderer } = new OverseerApp()
    const got = renderer.backdropLayers(gradient({ size: '18px', position: 'right top' }))
    expect(got.size).toBe('18px 18px')
    expect(got.position).toBe('right top')
    expect(got.repeat).toBe('no-repeat')
  })

  it('is as wide as its width says', () => {
    const { renderer } = new OverseerApp()
    expect(renderer.backdropLayers(gradient({ size: '18px', width: '40px' })).size).toBe('40px 18px')
  })

  it('fills the box for a size that is no single length', () => {
    const { renderer } = new OverseerApp()
    expect(renderer.backdropLayers(gradient({ size: '18px, url(x)' })).size).toBe('auto')
  })

  it('keeps its size as it turns, instead of being drawn across the box', () => {
    const { renderer } = new OverseerApp()
    const got = renderer.backdropLayers(gradient({ size: '18px', motion: 'turn' }))
    expect(got.layers[0].sized).toBe(true)
  })
})
