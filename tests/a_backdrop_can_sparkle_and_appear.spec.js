import { describe, it, expect, beforeEach, vi } from 'vitest'

// Two more ways a backdrop layer moves: `appear`, fading in once and staying, and `sparkle`, an icon
// layer drawn as small copies of its icons - one or several, or every library icon a `*` names -
// each at a place of its own, fading in and out. The page's own pages, shown while a document loads
// and when something goes wrong, are documents drawn with them.

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

describe('a backdrop layer that sparkles or appears', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
    globalThis.fetch = vi.fn(async (url) => {
      if (url === '/icons/index.json') return { ok: true, json: async () => ['eye', 'eye-closed', 'eye-off', 'eyeglass', 'star'] }
      const name = /^\/icons\/(.+)\.svg$/.exec(url)?.[1]
      return name ? { ok: true, text: async () => SVG(name) } : { ok: false, text: async () => '' }
    })
  })

  it('draws the icons a `*` and a list name, and paints nothing as a background', async () => {
    const app = new OverseerApp()
    const { image, layers } = await layersOf(app.renderer, block('backdrop', [
      icon('watchers', { source: 'eye-*, star', motion: 'sparkle', count: 5, size: '4%', opacity: 0.3, colour: '#ff0000' }),
    ]))
    expect(image).toBe('none')
    expect(layers).toHaveLength(1)
    const [sparkle] = layers
    expect(sparkle.motion).toBe('sparkle')
    expect(sparkle.period).toBe(6)
    expect(sparkle.sparkle.count).toBe(5)
    expect(sparkle.sparkle.size).toBe('4%')
    const drawn = sparkle.sparkle.images.map((css) => decodeURIComponent(/data:image\/svg\+xml,([^"]*)/.exec(css)[1]))
    // `eye-*` is the icons whose names start so: not `eye` itself, nor `eyeglass`.
    expect(drawn.map((svg) => /d="([^"]*)"/.exec(svg)[1])).toEqual(['eye-closed', 'eye-off', 'star'])
    expect(drawn.every((svg) => svg.includes('stroke="#ff0000"') && svg.includes('opacity="0.3"'))).toBe(true)
  })

  it('draws its copies in a box of their own, keeps them when drawn again, and draws them anew for another count', async () => {
    const app = new OverseerApp()
    const doc = (count) => [theme([block('backdrop', [
      icon('watchers', { source: 'eye-*', motion: 'sparkle', count, size: '4%', period: '3s' }),
    ])])]
    await layersOf(app.renderer, doc(5)[0].children[0])
    app.renderer.applyDocumentContexts(doc(5))
    await settle()
    const box = holderOf(document.body).children[0]
    expect(box.className).toBe('ov-backdrop-layer ov-backdrop-sparkles')
    expect(box.style.getPropertyValue('--ov-backdrop-period')).toBe('3s')
    const copies = Array.from(box.children)
    expect(copies).toHaveLength(5)
    expect(box.style.getPropertyValue('--ov-sparkle-side')).toBe('4cqh')
    expect(copies.every((c) => c.tagName === 'I')).toBe(true)
    expect(copies.every((c) => /^url\("data:image\/svg\+xml,/.test(c.style.backgroundImage))).toBe(true)
    expect(copies.every((c) => parseFloat(c.style.animationDelay) <= 0)).toBe(true)

    app.renderer.applyDocumentContexts(doc(5))
    await settle()
    expect(Array.from(holderOf(document.body).children[0].children)).toEqual(copies)

    app.renderer.applyDocumentContexts(doc(2))
    await settle()
    expect(holderOf(document.body).children[0].children).toHaveLength(2)
  })

  it('makes a container that only adds a sparkle a backdrop of its own', async () => {
    const app = new OverseerApp()
    const sparkles = block('backdrop', [icon('watchers', { source: 'star', motion: 'sparkle' })])
    await layersOf(app.renderer, sparkles)
    app.currentDocument = [div('page', [theme([sparkles]), field('a')])]
    app.renderer.renderDocument(app.currentDocument)
    const page = document.querySelector(`[data-path='${JSON.stringify(['page'])}']`)
    expect(page.classList.contains('ov-themed-backdrop')).toBe(true)
    expect(holderOf(page).children[0].children).toHaveLength(12)
  })

  it('fades a layer in once over its period, four seconds where it says none', async () => {
    const app = new OverseerApp()
    const backdrop = block('backdrop', [icon('eye', { source: 'eye', motion: 'appear' })])
    const { layers } = await layersOf(app.renderer, backdrop)
    expect(layers.map((l) => [l.name, l.motion, l.period])).toEqual([['eye', 'appear', 4]])
    app.renderer.applyDocumentContexts([theme([backdrop])])
    await settle()
    const box = holderOf(document.body).children[0]
    expect(box.className).toBe('ov-backdrop-layer ov-backdrop-appear')
    expect(box.style.getPropertyValue('--ov-backdrop-period')).toBe('4s')
  })
})

describe('the page\'s own pages', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
    globalThis.fetch = vi.fn(async () => ({ ok: false, text: async () => '', json: async () => [] }))
  })

  it('shows the loading page as a document, its name in the middle', () => {
    const app = new OverseerApp()
    app.showLoadingScreen()
    expect(document.getElementById('loading-screen').classList.contains('active')).toBe(true)
    expect(document.getElementById('loading-display').textContent).toContain('OVERSEER')
    // Drawn into its own screen: the document's content is left alone.
    expect(document.getElementById('content-display').textContent).toBe('')
  })

  it('shows an error on the error page, what failed and why', () => {
    const app = new OverseerApp()
    vi.spyOn(console, 'error').mockImplementation(() => {})
    app.showError('Failed to load file', new Error('The server could not be reached.'))
    expect(document.getElementById('error-screen').classList.contains('active')).toBe(true)
    const shown = document.getElementById('error-display').textContent
    expect(shown).toContain('Failed to load file')
    expect(shown).toContain('The server could not be reached.')
    // The plain text beside it is what shows should the page fail to draw.
    const said = document.getElementById('error-message')
    expect(said.textContent).toBe('Failed to load file: The server could not be reached.')
    expect(said.hidden).toBe(true)
  })
})
