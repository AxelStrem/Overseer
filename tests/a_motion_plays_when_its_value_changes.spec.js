import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// `motion=` names a motion. One played once plays when the worked-out value changes to it, never
// because the node was drawn again - a press elsewhere redraws whole containers, and every flag
// in a minesweeper would pop again at each dig. The value last seen is remembered by address, so
// a first draw plays nothing. `pulse` runs while the value names it. The theme's `motion = none`
// stills it all. A hold under way is drawn as the theme's `hold/style` says, and goes with it.

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

/// The cell's motion as a formula the resolver has worked out to `motion`.
const cell = (motion, extra = {}) => base('cell', 'div', {
  motion: { Formula: 'flagged ? "pop" : ""' }, _computed_motion: { String: motion }, ...extra,
}, [base('x', 'bool', { value: { Boolean: false } })])

const theDocument = (motion, { other = 0, themed = [], cellExtra = {}, cellChildren = [] } = {}) => [
  base('s', 'tab', { mutable: { Boolean: true } }, [
    ...themed,
    base('count', 'int', { value: { Integer: other } }),
    { ...cell(motion, cellExtra), children: [...cell(motion).children, ...cellChildren] },
  ]),
]

const at = () => document.querySelector(`[data-path='${JSON.stringify(['s', 'cell'])}']`)
const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

let app
const draw = async (doc) => {
  app.currentDocument = doc
  app.renderer.renderDocument(doc)
  await settled()
}

describe('a motion', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
    app = new OverseerApp()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
  })

  it('plays nothing on the first draw', async () => {
    await draw(theDocument('pop'))
    expect(at().classList.contains('ov-motion-pop')).toBe(false)
  })

  it('plays once when its value changes to it', async () => {
    await draw(theDocument(''))
    await draw(theDocument('pop'))
    expect(at().classList.contains('ov-motion-pop')).toBe(true)
  })

  it('does not play again when the node is drawn again for something else', async () => {
    await draw(theDocument(''))
    await draw(theDocument('pop'))
    await draw(theDocument('pop', { other: 1 }))
    expect(at().classList.contains('ov-motion-pop')).toBe(false)
  })

  it('plays again for a new value after a colon', async () => {
    await draw(theDocument('shake:1'))
    await draw(theDocument('shake:2'))
    expect(at().classList.contains('ov-motion-shake')).toBe(true)
  })

  it('throws dots for a burst, and takes them away once it has run', async () => {
    await draw(theDocument(''))
    vi.useFakeTimers({ toFake: ['setTimeout'] })
    try {
      app.currentDocument = theDocument('burst')
      app.renderer.renderDocument(app.currentDocument)
      await Promise.resolve()
      expect(at().querySelectorAll(':scope > .ov-burst > i').length).toBe(8)
      vi.advanceTimersByTime(1000)
      expect(at().querySelector('.ov-burst')).toBeNull()
      expect(at().classList.contains('ov-motion-burst')).toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })

  it('runs a pulse for as long as its value says so', async () => {
    await draw(theDocument('pulse'))
    expect(at().classList.contains('ov-motion-pulse')).toBe(true)
    await draw(theDocument(''))
    expect(at().classList.contains('ov-motion-pulse')).toBe(false)
  })

  it('moves nothing for a name it does not know', async () => {
    await draw(theDocument(''))
    await draw(theDocument('wobble'))
    expect(Array.from(at().classList).some((c) => c.startsWith('ov-motion-'))).toBe(false)
  })

  it('is stilled by the theme', async () => {
    const themed = [theme(key('motion', 'none'))]
    await draw(theDocument('', { themed }))
    await draw(theDocument('pop', { themed }))
    expect(at().classList.contains('ov-motion-pop')).toBe(false)
  })
})

describe('a hold under way', () => {
  const pointer = (element, type) => {
    const event = new window.MouseEvent(type, { bubbles: true, cancelable: true, clientX: 10, clientY: 10 })
    Object.defineProperty(event, 'pointerType', { value: 'touch' })
    Object.defineProperty(event, 'pointerId', { value: 1 })
    element.dispatchEvent(event)
  }
  const holdTheme = (style) => [theme({ ...base('hold', '-', {}, [key('style', style)]) })]
  const onHold = [base('hold', 'on', {}, [base('toggle', 'toggle', { path: { String: './x' } })])]

  beforeEach(() => {
    vi.useFakeTimers()
    setupDOM()
    document.documentElement.removeAttribute('style')
    app = new OverseerApp()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    vi.spyOn(app.renderer, 'emitEvent').mockImplementation(async () => {})
  })
  afterEach(() => vi.useRealTimers())

  const drawn = (style) => {
    app.currentDocument = theDocument('', { themed: holdTheme(style), cellExtra: { hold: { String: '2s' } }, cellChildren: onHold })
    app.renderer.renderDocument(app.currentDocument)
  }

  it('fills the node over the hold time, and the fill goes when the press is let go', () => {
    drawn('fill')
    pointer(at(), 'pointerdown')
    const fill = at().querySelector(':scope > .ov-hold.ov-hold-fill')
    expect(fill).not.toBeNull()
    expect(fill.style.getPropertyValue('--ov-hold-time')).toBe('2000ms')
    vi.advanceTimersByTime(500)
    pointer(at(), 'pointerup')
    expect(at().querySelector('.ov-hold')).toBeNull()
  })

  it('goes when the press moves off, and when it becomes the hold', () => {
    drawn('ring')
    pointer(at(), 'pointerdown')
    expect(at().querySelector('.ov-hold-ring')).not.toBeNull()
    pointer(at(), 'pointerleave')
    expect(at().querySelector('.ov-hold')).toBeNull()
    pointer(at(), 'pointerdown')
    vi.advanceTimersByTime(2000)
    expect(at().querySelector('.ov-hold')).toBeNull()
  })

  it('draws nothing where the theme says none', () => {
    drawn('none')
    pointer(at(), 'pointerdown')
    expect(at().querySelector('.ov-hold')).toBeNull()
  })
})
