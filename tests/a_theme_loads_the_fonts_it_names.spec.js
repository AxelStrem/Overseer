import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A theme can name font files to load, `- fonts { - plex_sans = "fonts/plex-sans.woff2" }`: the
// page asks for each with `load_overseer_font`, adds it to the page under the family's name, and
// takes it off when no theme drawn names it any more. jsdom has no `FontFace` or `document.fonts`,
// so both are stood in for here; whether the text is drawn in the font is seen in the real page.

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

const asked = []
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd, args) => {
    if (cmd !== 'load_overseer_font') return null
    asked.push(args.font)
    if (args.font === 'fonts/none.woff2') throw new Error('no font')
    return { mime: 'font/woff2', data: btoa('wOF2') }
  }),
}))
import { OverseerApp } from '../src/main.js'

class StandInFace {
  constructor(family, bytes) {
    this.family = family
    this.bytes = bytes
  }
  load() { return Promise.resolve(this) }
}

const base = (name, type) => ({ name, node_type: type, parameters: {}, is_hierarchy_transparent: false, children: [] })
const key = (name, value) => Object.assign(base(name, '-'), { parameters: { value: { String: value } } })
const group = (name, ...children) => Object.assign(base(name, '-'), { children })
const theme = (...children) => Object.assign(base('theme', 'context'), {
  children, parameters: { _context_copy: { Boolean: true } },
})
const field = Object.assign(base('note', 'string'), { parameters: { value: { String: 'hello' } } })

const settled = () => new Promise((resolve) => setTimeout(resolve, 0))
const families = () => Array.from(document.fonts).map((face) => face.family).sort()

describe('a theme that names fonts to load', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
    asked.length = 0
    globalThis.FontFace = StandInFace
    Object.defineProperty(document, 'fonts', { value: new Set(), configurable: true })
  })

  afterEach(() => {
    delete globalThis.FontFace
    delete document.fonts
  })

  it('adds each font it names to the page under its family, fetched once', async () => {
    const app = new OverseerApp()
    const doc = () => [
      theme(key('font', 'plex_sans, sans-serif'),
        group('fonts', key('plex_sans', 'fonts/plex-sans.woff2'), key('plex_mono', 'fonts/plex-mono.woff2'))),
      field,
    ]
    app.renderer.renderDocument(doc())
    await settled()
    expect(families()).toEqual(['plex_mono', 'plex_sans'])
    expect(new TextDecoder().decode(Array.from(document.fonts)[0].bytes)).toBe('wOF2')

    app.renderer.renderDocument(doc())
    await settled()
    expect(families()).toEqual(['plex_mono', 'plex_sans'])
    expect(asked).toEqual(['fonts/plex-sans.woff2', 'fonts/plex-mono.woff2'])
  })

  it('writes no variables for its fonts', async () => {
    const app = new OverseerApp()
    app.renderer.renderDocument([theme(group('fonts', key('plex_sans', 'fonts/plex-sans.woff2'))), field])
    await settled()
    const said = Array.from(document.documentElement.style).filter((name) => name.includes('fonts'))
    expect(said).toEqual([])
  })

  it('takes a font off the page once the theme no longer names it', async () => {
    const app = new OverseerApp()
    app.renderer.renderDocument([theme(group('fonts', key('plex_sans', 'fonts/plex-sans.woff2'))), field])
    await settled()
    expect(families()).toEqual(['plex_sans'])
    const fetched = asked.length

    app.renderer.renderDocument([theme(group('fonts', key('serif', 'fonts/serif.ttf'))), field])
    await settled()
    expect(families()).toEqual(['serif'])

    app.renderer.renderDocument([theme(group('fonts', key('plex_sans', 'fonts/plex-sans.woff2'))), field])
    await settled()
    expect(families()).toEqual(['plex_sans'])
    expect(asked.slice(fetched)).not.toContain('fonts/plex-sans.woff2')
  })

  it('leaves out a font that is missing, and draws the rest', async () => {
    const app = new OverseerApp()
    app.renderer.renderDocument([
      theme(group('fonts', key('gone', 'fonts/none.woff2'), key('serif', 'fonts/serif.ttf'))),
      field,
    ])
    await settled()
    await settled()
    expect(families()).toEqual(['serif'])
    expect(document.body.textContent).toContain('hello')
  })
})
