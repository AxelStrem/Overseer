import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

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
})
