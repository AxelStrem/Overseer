import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// Cards of a canvas are picked with Ctrl, Cmd or Shift and a click, or with a band drawn on the bare
// area, and let go of with Escape or a click there. A picked card dragged carries the others by as
// much, and letting go writes every one of them in one `set_overseer_entry_fields`.

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

const theDocument = ({ mutable = true, xType = 'float', templateAt = [0, 0] } = {}) => [base('t', 'tab', { mutable: { Boolean: mutable } }, [
  base('Card', 'div', { hidden: { Boolean: true } }, [
    base('title', 'string', { value: { String: '' } }),
    base('x', xType, { value: xType === 'int' ? { Integer: templateAt[0] } : { Float: templateAt[0] }, hidden: { Boolean: true } }),
    base('y', 'float', { value: { Float: templateAt[1] }, hidden: { Boolean: true } }),
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
  // A card lifted at the frame's edge scrolls it, which jsdom does not do.
  scroller().scrollBy = vi.fn()
  area().getBoundingClientRect = () => rect(10, 100, 800, 500)
  for (const el of area().querySelectorAll(':scope > [data-uid]')) {
    boxes.set(el, rect(10 + parseFloat(el.style.left), 100 + parseFloat(el.style.top), 50, 40))
  }
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

const pointer = (target, type, x, y, keys = {}) => {
  const event = new window.MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, ...keys })
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

const click = (el, keys) => el.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true, ...keys }))
const picked = () => Array.from(area().querySelectorAll('.canvas-picked')).map((el) => JSON.parse(el.dataset.path).pop())

describe('several cards of a canvas', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    delete document.elementFromPoint
  })

  it('are picked and let go of by a click with Ctrl, Cmd or Shift held', () => {
    render()
    click(entryAt('placed'), { ctrlKey: true })
    click(entryAt('far'), { shiftKey: true })
    expect(picked()).toEqual(['placed', 'far'])
    click(entryAt('placed'), { metaKey: true })
    expect(picked()).toEqual(['far'])
  })

  it('are not picked by a plain click', () => {
    render()
    click(entryAt('placed'))
    expect(picked()).toEqual([])
  })

  it('are let go of by Escape and by a click on the bare area', () => {
    render()
    layOut()
    click(entryAt('placed'), { ctrlKey: true })
    window.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape' }))
    expect(picked()).toEqual([])
    click(entryAt('placed'), { ctrlKey: true })
    pointer(area(), 'pointerdown', 300, 350)
    pointer(window, 'pointerup', 300, 350)
    expect(picked()).toEqual([])
  })

  it('are picked by a band drawn on the bare area, every card it touches', () => {
    render()
    layOut()
    // From (5, 5) on the area to (140, 60): over 'loose' at 0 and 'placed' at 120, short of 'far'.
    pointer(area(), 'pointerdown', 15, 105)
    pointer(window, 'pointermove', 60, 130)
    expect(area().querySelector('.canvas-band')).not.toBeNull()
    pointer(window, 'pointermove', 150, 160)
    pointer(window, 'pointerup', 150, 160)
    expect(picked().sort()).toEqual(['loose', 'placed'])
    expect(area().querySelector('.canvas-band')).toBeNull()
  })

  it('stay picked across the canvas being drawn again', () => {
    render()
    click(entryAt('placed'), { ctrlKey: true })
    app.renderer.renderDocument(app.currentDocument)
    expect(picked()).toEqual(['placed'])
  })

  it('move together, written as one instruction', async () => {
    render()
    layOut()
    click(entryAt('placed'), { ctrlKey: true })
    click(entryAt('far'), { ctrlKey: true })
    // 'placed' from 120, 40 to 170, 60: 50 right and 20 down.
    dragTo(entryAt('placed'), 10 + 170 + 2, 100 + 60 + 2)
    await settle()
    const calls = placings()
    expect(calls).toHaveLength(1)
    expect(calls[0].entries).toEqual([
      { entry_path: ['t', 'Notes', 'placed'], settings: [
        { node_path: ['x'], value: { Float: 170 } }, { node_path: ['y'], value: { Float: 60 } }] },
      { entry_path: ['t', 'Notes', 'far'], settings: [
        { node_path: ['x'], value: { Float: 650 } }, { node_path: ['y'], value: { Float: 320 } }] },
    ])
    expect(entryAt('far').style.left).toBe('650px')
  })

  it('keep their shape against the area\'s top and left edge', async () => {
    render()
    layOut()
    click(entryAt('placed'), { ctrlKey: true })
    click(entryAt('far'), { ctrlKey: true })
    // 'placed', held 40 by 30 in, let go at the frame's corner: it would stand past the edge, so
    // it stops there, and 'far' stops as far.
    const r = entryAt('placed').getBoundingClientRect()
    pointer(entryAt('placed'), 'pointerdown', r.left + 40, r.top + 30)
    pointer(window, 'pointermove', r.left + 40, r.top + 40)
    pointer(window, 'pointermove', 12, 102)
    pointer(window, 'pointerup', 12, 102)
    await settle()
    const [{ entries }] = placings()
    expect(entries.map((e) => e.settings.map((s) => s.value.Float))).toEqual([[0, 0], [480, 260]])
  })

  it('carry the others along while dragged, and put them back when the drag comes to nothing', () => {
    render()
    layOut()
    click(entryAt('placed'), { ctrlKey: true })
    click(entryAt('far'), { ctrlKey: true })
    const r = entryAt('placed').getBoundingClientRect()
    pointer(entryAt('placed'), 'pointerdown', r.left + 2, r.top + 2)
    pointer(window, 'pointermove', r.left + 2, r.top + 12)
    pointer(window, 'pointermove', r.left + 32, r.top + 12)
    expect(entryAt('far').style.left).toBe('630px')
    expect(entryAt('far').style.top).toBe('310px')
    window.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape' }))
    expect(entryAt('far').style.left).toBe('600px')
    expect(entryAt('far').style.top).toBe('300px')
    expect(placings()).toHaveLength(0)
  })

  it('stay put when a card not picked is dragged, which goes alone and lets them go', async () => {
    render()
    layOut()
    click(entryAt('far'), { ctrlKey: true })
    dragTo(entryAt('placed'), 10 + 170 + 2, 100 + 60 + 2)
    await settle()
    const [args] = placings()
    expect(args.entries).toBeUndefined()
    expect(args.entry_path).toEqual(['t', 'Notes', 'placed'])
    expect(entryAt('far').style.left).toBe('600px')
    expect(picked()).toEqual([])
  })
})
