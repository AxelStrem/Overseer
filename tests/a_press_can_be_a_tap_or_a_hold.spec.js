import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A node that says `on hold` tells a tap from a press held down: a minesweeper cell flags on a
// tap and digs on a hold, which a phone can do without a right-click. A press held still for the
// node's `hold=` time runs `on hold`, and the click the browser sends when it is let go does not
// run `on click` as well; one let go sooner is a tap. Moving off it, or a scroll taking it over,
// is neither. A right-click is a hold at once, and so is Enter held down.

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

const on = (event) => base(event, 'on', {}, [base('toggle', 'toggle', { path: { String: './x' } })])

const theDocument = () => [base('s', 'tab', { mutable: { Boolean: true } }, [
  base('cell', 'div', { hold: { String: '2s' } }, [base('x', 'bool', { value: { Boolean: false } }), on('click'), on('hold')]),
  base('quick', 'div', {}, [on('click'), on('hold')]),
  base('digger', 'div', { hold: { Integer: 1 } }, [on('hold')]),
  base('tapper', 'div', {}, [on('click')]),
  base('dig', 'button', { label: { String: 'dig' }, hold: { String: '500ms' } }, [on('click'), on('hold')]),
])]

let app
let ran

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  ran = []
  vi.spyOn(app.renderer, 'emitEvent').mockImplementation(async (node, _el, eventName) => {
    if (app.renderer.declaresEvent(node, eventName)) ran.push(`${node.name} ${eventName}`)
  })
  app.renderer.renderDocument(app.currentDocument)
}

const at = (name) => document.querySelector(`[data-path='${JSON.stringify(['s', name])}']`)

const pointer = (element, type, { x = 10, y = 10, kind = 'touch', button = 0 } = {}) => {
  const event = new window.MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button })
  Object.defineProperty(event, 'pointerType', { value: kind })
  Object.defineProperty(event, 'pointerId', { value: 1 })
  element.dispatchEvent(event)
  return event
}

/// A press as a finger makes one: down, held for so long, up, and the click the browser sends.
const press = (element, ms, options = {}) => {
  pointer(element, 'pointerdown', options)
  vi.advanceTimersByTime(ms)
  pointer(element, 'pointerup', options)
  element.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))
}

const key = (element, type, name, repeat = false) =>
  element.dispatchEvent(new window.KeyboardEvent(type, { key: name, bubbles: true, cancelable: true, repeat }))

describe('a press can be a tap or a hold', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setupDOM()
    render()
  })
  afterEach(() => vi.useRealTimers())

  it('is a tap when let go before the hold time', () => {
    press(at('cell'), 1900)
    expect(ran).toEqual(['cell click'])
  })

  it('is a hold when held for the hold time, and the click that ends it is not a tap', () => {
    press(at('cell'), 2000)
    expect(ran).toEqual(['cell hold'])
  })

  it('runs the hold while it is still held down', () => {
    pointer(at('cell'), 'pointerdown')
    vi.advanceTimersByTime(2000)
    expect(ran, 'a hold waited for the finger to lift').toEqual(['cell hold'])
  })

  it('waits a little over half a second where the node gives no time', () => {
    press(at('quick'), 500)
    press(at('quick'), 600)
    expect(ran).toEqual(['quick click', 'quick hold'])
  })

  it('reads a time given as a number as seconds', () => {
    press(at('digger'), 900)
    press(at('digger'), 1000)
    expect(ran).toEqual(['digger hold'])
  })

  it('is neither when the press moves off the node', () => {
    const cell = at('cell')
    pointer(cell, 'pointerdown')
    vi.advanceTimersByTime(500)
    pointer(cell, 'pointerleave')
    vi.advanceTimersByTime(3000)
    cell.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))
    expect(ran).toEqual([])
  })

  it('is neither when the finger moves away on it', () => {
    const cell = at('cell')
    pointer(cell, 'pointerdown')
    pointer(cell, 'pointermove', { x: 40 })
    vi.advanceTimersByTime(3000)
    pointer(cell, 'pointerup', { x: 40 })
    cell.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))
    expect(ran).toEqual([])
  })

  it('is neither when a scroll takes the press over', () => {
    const cell = at('cell')
    pointer(cell, 'pointerdown')
    pointer(cell, 'pointercancel')
    vi.advanceTimersByTime(3000)
    expect(ran).toEqual([])
  })

  it('is a tap again on the next press after one that was neither', () => {
    const cell = at('cell')
    pointer(cell, 'pointerdown')
    pointer(cell, 'pointercancel')
    press(cell, 100)
    expect(ran).toEqual(['cell click'])
  })

  it('is a hold at once on a right-click', () => {
    const menu = pointer(at('cell'), 'contextmenu', { kind: 'mouse', button: 2 })
    expect(ran).toEqual(['cell hold'])
    expect(menu.defaultPrevented, 'the browser menu opened too').toBe(true)
  })

  it('leaves a finger held down to its timer when a phone asks for its menu', () => {
    const cell = at('cell')
    pointer(cell, 'pointerdown')
    vi.advanceTimersByTime(500)
    pointer(cell, 'contextmenu')
    expect(ran, 'the phone menu made it a hold early').toEqual([])
    vi.advanceTimersByTime(1500)
    expect(ran).toEqual(['cell hold'])
  })

  it('is a hold when Enter is held, and a tap when it is let go sooner', () => {
    const cell = at('cell')
    key(cell, 'keydown', 'Enter')
    key(cell, 'keyup', 'Enter')
    key(cell, 'keydown', 'Enter')
    vi.advanceTimersByTime(1000)
    key(cell, 'keydown', 'Enter', true)
    vi.advanceTimersByTime(1000)
    key(cell, 'keyup', 'Enter')
    expect(ran).toEqual(['cell click', 'cell hold'])
  })

  it('makes a node with only on hold pressable, and a tap on it does nothing', () => {
    const digger = at('digger')
    expect(digger.classList.contains('overseer-pressable')).toBe(true)
    press(digger, 100)
    expect(ran).toEqual([])
  })

  it('leaves a node without on hold tapped at once', () => {
    const tapper = at('tapper')
    expect(tapper.dataset.holds).toBeUndefined()
    press(tapper, 5000)
    key(tapper, 'keydown', 'Enter')
    expect(ran).toEqual(['tapper click', 'tapper click'])
  })

  it('tells a tap from a hold on a button too', () => {
    const button = at('dig').querySelector('button') || at('dig')
    press(button, 100, { kind: 'mouse' })
    press(button, 500, { kind: 'mouse' })
    expect(ran).toEqual(['dig click', 'dig hold'])
  })
})
