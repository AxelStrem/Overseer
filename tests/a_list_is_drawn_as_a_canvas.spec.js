import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A list that says `view="canvas", x="x", y="y"` draws its entries as cards on an open area, each
// where its two fields say. One whose fields still hold the template's values waits in a strip
// above the area. A card let go on the area gives its entry both fields as one instruction,
// `set_overseer_entry_fields`, named from the entry, and moves nothing in the list.

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
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const card = (title, x, y, xType = 'float') => base(title, 'div', {}, [
  base('title', 'string', { value: { String: title } }),
  base('x', xType, { value: xType === 'int' ? { Integer: x } : { Float: x }, hidden: { Boolean: true } }),
  base('y', 'float', { value: { Float: y }, hidden: { Boolean: true } }),
])

const theDocument = ({ mutable = true, xType = 'float' } = {}) => [base('t', 'tab', { mutable: { Boolean: mutable } }, [
  base('Card', 'div', { hidden: { Boolean: true } }, [
    base('title', 'string', { value: { String: '' } }),
    base('x', xType, { value: xType === 'int' ? { Integer: 0 } : { Float: 0 }, hidden: { Boolean: true } }),
    base('y', 'float', { value: { Float: 0 }, hidden: { Boolean: true } }),
  ]),
  base('Notes', 'list', {
    entry: { Template: '<Card>' }, key: { String: 'title' },
    view: { String: 'canvas' }, x: { String: 'x' }, y: { String: 'y' },
  }, [card('placed', 120, 40, xType), card('loose', 0, 0, xType), card('far', 600, 300, xType)]),
])]

let app

const render = (options) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(options)
  app.currentFile = 'd.os'
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

const list = () => document.querySelector(`.overseer-list[data-path='${JSON.stringify(['t', 'Notes'])}']`)
const area = () => list().querySelector('.canvas-area')
const strip = () => list().querySelector('.canvas-strip')
const scroller = () => list().querySelector('.canvas-scroll')
const entryAt = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', 'Notes', name])}']`)
const placings = () => invoke.mock.calls.filter(([cmd]) => cmd === 'set_overseer_entry_fields').map(([, args]) => args)

const rect = (left, top, width, height) => ({
  left, top, width, height, right: left + width, bottom: top + height, x: left, y: top,
})

/// The scroll frame at (10, 100), 400 by 300, its area scrolled 0; each card 50 by 40 where it stands.
const layOut = () => {
  const boxes = new Map()
  boxes.set(scroller(), rect(10, 100, 400, 300))
  area().getBoundingClientRect = () => rect(10, 100, 800, 500)
  for (const el of area().querySelectorAll(':scope > [data-uid]')) {
    boxes.set(el, rect(10 + parseFloat(el.style.left), 100 + parseFloat(el.style.top), 50, 40))
  }
  Array.from(strip().children).forEach((el, k) => boxes.set(el, rect(10 + k * 60, 40, 50, 40)))
  for (const [el, box] of boxes) el.getBoundingClientRect = () => box
  document.elementFromPoint = (x, y) => {
    let found = null
    for (const [el, box] of boxes) {
      if (x >= box.left && x < box.right && y >= box.top && y < box.bottom) {
        if (!found || found.contains(el)) found = el
      }
    }
    return found
  }
}

const pointer = (target, type, x, y) => {
  const event = new window.MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 })
  Object.defineProperty(event, 'pointerId', { value: 1 })
  Object.defineProperty(event, 'pointerType', { value: 'mouse' })
  target.dispatchEvent(event)
}

/// Press on a card 2 pixels in from its corner, carry it, and let go at a point.
const dragTo = (from, x, y) => {
  const r = from.getBoundingClientRect()
  pointer(from, 'pointerdown', r.left + 2, r.top + 2)
  pointer(window, 'pointermove', r.left + 2, r.top + 12)
  pointer(window, 'pointermove', x, y)
  pointer(window, 'pointerup', x, y)
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('a list drawn as a canvas', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    delete document.elementFromPoint
  })

  it('stands each placed entry where its fields say', () => {
    render()
    expect(list().classList.contains('view-canvas')).toBe(true)
    expect(entryAt('placed').parentElement).toBe(area())
    expect(entryAt('placed').style.left).toBe('120px')
    expect(entryAt('placed').style.top).toBe('40px')
    expect(entryAt('far').style.left).toBe('600px')
  })

  it('keeps an entry still at the template\'s values in the strip, not under another', () => {
    render()
    expect(entryAt('loose').parentElement).toBe(strip())
    expect(strip().hidden).toBe(false)
  })

  it('grows its area to hold the farthest card', () => {
    render()
    expect(parseFloat(area().style.width)).toBeGreaterThan(600)
    expect(parseFloat(area().style.height)).toBeGreaterThan(300)
  })

  it('places a dragged card where it is let go, both fields in one instruction named from the entry', async () => {
    render()
    layOut()
    // Grabbed 2 pixels in from its corner, let go at (312, 252): its corner lands at (300, 150).
    dragTo(entryAt('placed'), 312, 252)
    await vi.waitFor(() => expect(placings()).toHaveLength(1))
    expect(placings()[0]).toMatchObject({
      entry_path: ['t', 'Notes', 'placed'],
      settings: [
        { node_path: ['x'], value: { Float: 300 } },
        { node_path: ['y'], value: { Float: 150 } },
      ],
    })
    expect(placings()[0].settings[0].nodePath).toBeUndefined()
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'move_overseer_entry')).toBe(false)
  })

  it('draws a placed card being dragged under the pointer, held where it was grabbed', () => {
    render()
    layOut()
    // Placed at (120, 40) on the area, so its corner is at (130, 140) on the screen.
    const from = entryAt('placed')
    pointer(from, 'pointerdown', 132, 142)
    pointer(window, 'pointermove', 132, 152)
    pointer(window, 'pointermove', 250, 200)
    const shell = document.querySelector('.overseer-drag-shell')
    const ghost = shell.querySelector('.overseer-drag-ghost')
    const { left, top } = ghost.style
    pointer(window, 'pointerup', 250, 200)
    expect(shell.style.left).toBe('248px')
    expect(shell.style.top).toBe('198px')
    // The card's own place on the area would push the copy that much further from the pointer.
    expect(left).toBe('')
    expect(top).toBe('')
  })

  it('gives an entry from the strip a place once it is dragged onto the area', async () => {
    render()
    layOut()
    dragTo(entryAt('loose'), 52, 142)
    await vi.waitFor(() => expect(placings()).toHaveLength(1))
    expect(placings()[0].entry_path).toEqual(['t', 'Notes', 'loose'])
    expect(placings()[0].settings.map((s) => s.value)).toEqual([{ Float: 40 }, { Float: 40 }])
  })

  it('writes a whole number to an int field', async () => {
    render({ xType: 'int' })
    layOut()
    dragTo(entryAt('placed'), 312, 252)
    await vi.waitFor(() => expect(placings()).toHaveLength(1))
    expect(placings()[0].settings[0].value).toEqual({ Integer: 300 })
  })

  it('writes nothing for a card let go outside the area', async () => {
    render()
    layOut()
    dragTo(entryAt('placed'), 900, 900)
    await settle()
    expect(placings()).toHaveLength(0)
  })

  it('offers no drag where the document is read-only', async () => {
    render({ mutable: false })
    layOut()
    expect(list().dataset.canvasDrag).toBeUndefined()
    dragTo(entryAt('placed'), 312, 252)
    await settle()
    expect(placings()).toHaveLength(0)
  })

  it('draws the card last pressed in above the others, and only that one', () => {
    render()
    pointer(entryAt('placed').querySelector('[data-path]') || entryAt('placed'), 'pointerdown', 0, 0)
    expect(entryAt('placed').classList.contains('canvas-front')).toBe(true)
    pointer(entryAt('far'), 'pointerdown', 0, 0)
    expect(entryAt('far').classList.contains('canvas-front')).toBe(true)
    expect(entryAt('placed').classList.contains('canvas-front')).toBe(false)
  })

  it('keeps the card last pressed in above the others when the document is drawn again', () => {
    render()
    pointer(entryAt('placed'), 'pointerdown', 0, 0)
    app.currentDocument = theDocument()
    app.renderer.renderDocument(app.currentDocument)
    expect(entryAt('placed').classList.contains('canvas-front')).toBe(true)
    expect(document.querySelectorAll('.canvas-front')).toHaveLength(1)
  })

  it('stacks a canvas card that is pointed at, edited or last pressed in above the cards around it', () => {
    const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
    // The z-index of the rule whose selectors name the given one.
    const zIndexOf = (selector) => {
      for (const [, selectors, body] of styles.matchAll(/([^{}]+)\{([^}]*)\}/g)) {
        if (!selectors.split(',').some((s) => s.trim() === selector)) continue
        const z = body.match(/z-index:\s*(\d+)/)
        if (z) return Number(z[1])
      }
      return null
    }
    expect(zIndexOf('.canvas-area > [data-uid].canvas-front')).toBeGreaterThan(0)
    expect(zIndexOf('.canvas-area > [data-uid]:hover')).toBeGreaterThan(zIndexOf('.canvas-area > [data-uid].canvas-front'))
    expect(zIndexOf('.canvas-area > [data-uid]:focus-within')).toBeGreaterThan(zIndexOf('.canvas-area > [data-uid].canvas-front'))
  })

  it('draws a placed card at its place, whatever margin the card is given inline', () => {
    const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
    const bodyOf = (selector) => [...styles.matchAll(/([^{}]+)\{([^}]*)\}/g)]
      .filter(([, selectors]) => selectors.split(',').some((s) => s.trim() === selector))
      .map(([, , body]) => body)
      .join(';')
    // A field's default margin is written inline, which only an important rule outweighs: kept, it
    // would draw the card that far below the place it was let go at, and every drag would creep.
    expect(bodyOf('.canvas-area > [data-uid]')).toMatch(/margin:\s*0(px)?\s*!important/)
  })

  it('keeps the strip to one row that scrolls sideways, its cards at their own width', () => {
    const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
    // Everything the rules naming the given selector say, in order.
    const bodyOf = (selector) => [...styles.matchAll(/([^{}]+)\{([^}]*)\}/g)]
      .filter(([, selectors]) => selectors.split(',').some((s) => s.trim() === selector))
      .map(([, , body]) => body)
      .join(';')
    expect(bodyOf('.canvas-strip')).toMatch(/flex-wrap:\s*nowrap/)
    expect(bodyOf('.canvas-strip')).toMatch(/overflow-x:\s*auto/)
    expect(bodyOf('.canvas-strip > [data-uid]')).toMatch(/flex:\s*none/)
  })
})
