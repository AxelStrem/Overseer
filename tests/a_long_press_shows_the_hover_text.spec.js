import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A finger cannot rest on a field without pressing it, so on a touch screen a long press shows
// what `hover-text` says, the click that ends it does not run what was held, and the next touch
// puts it away. A node with `on hold` keeps the long press for its own action, and a tap's
// leftover mouse events show nothing.

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

const theDocument = () => [base('t', 'tab', {}, [
  base('weight', 'string', { value: { String: '80' }, label: { String: 'Weight' }, 'hover-text': { String: 'In kg' } }),
  base('rem', 'button', { icon: { String: 'trash-2' }, 'hover-text': { String: 'Remove this record' } }, [on('click')]),
  base('dig', 'button', { label: { String: 'dig' }, 'hover-text': { String: 'Dig here' } }, [on('click'), on('hold')]),
])]

let ran

const render = () => {
  const app = new OverseerApp()
  app.currentDocument = theDocument()
  app._currentText = 'TEXT'
  ran = []
  vi.spyOn(app.renderer, 'emitEvent').mockImplementation(async (node, _el, eventName) => {
    if (app.renderer.declaresEvent(node, eventName)) ran.push(`${node.name} ${eventName}`)
  })
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const at = (name) => {
  const found = document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)
  return found?.matches('[data-hover-text]') ? found : found?.querySelector('[data-hover-text]') || found
}

const pointer = (element, type, { x = 10, y = 10, kind = 'touch' } = {}) => {
  const event = new window.MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y })
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

const popup = () => {
  const shown = document.querySelector('.overseer-hover-text')
  return shown && shown.style.display !== 'none' ? shown.textContent : null
}

describe('a long press shows the hover text', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setupDOM()
    render()
  })
  afterEach(() => {
    vi.useRealTimers()
    document.querySelectorAll('.overseer-hover-text').forEach((n) => n.remove())
  })

  it('shows what a field says when it is held', () => {
    pointer(at('weight'), 'pointerdown')
    vi.advanceTimersByTime(500)
    expect(popup()).toBe('In kg')
  })

  it('shows what an icon button says, and the click that ends the press does not run it', () => {
    press(at('rem'), 600)
    expect(popup()).toBe('Remove this record')
    expect(ran).toEqual([])
  })

  it('leaves a tap a tap, showing nothing', () => {
    press(at('rem'), 100)
    vi.advanceTimersByTime(1000)
    expect(ran).toEqual(['rem click'])
    expect(popup()).toBeNull()
  })

  it('swallows only the click that ends the long press, and not the next tap', () => {
    press(at('rem'), 600)
    press(at('rem'), 100)
    expect(ran).toEqual(['rem click'])
  })

  it('leaves the long press to a node that holds', () => {
    press(at('dig'), 700)
    expect(ran).toEqual(['dig hold'])
    expect(popup()).toBeNull()
  })

  it('shows nothing when the finger moves away', () => {
    const weight = at('weight')
    pointer(weight, 'pointerdown')
    pointer(weight, 'pointermove', { x: 40 })
    vi.advanceTimersByTime(1000)
    expect(popup()).toBeNull()
  })

  it('shows nothing when a scroll takes the press over', () => {
    const weight = at('weight')
    pointer(weight, 'pointerdown')
    pointer(weight, 'pointercancel')
    vi.advanceTimersByTime(1000)
    expect(popup()).toBeNull()
  })

  it('goes away at the next touch anywhere', () => {
    press(at('weight'), 600)
    expect(popup()).toBe('In kg')
    pointer(document.getElementById('status-bar'), 'pointerdown')
    expect(popup()).toBeNull()
  })

  it('keeps the menu a phone offers away from a long press', () => {
    const rem = at('rem')
    pointer(rem, 'pointerdown')
    const menu = new window.MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    rem.dispatchEvent(menu)
    expect(menu.defaultPrevented).toBe(true)
  })

  it('is not shown by the mouse events a tap leaves behind', () => {
    const weight = at('weight')
    pointer(weight, 'pointerdown')
    pointer(weight, 'pointerup')
    weight.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
    vi.advanceTimersByTime(1000)
    expect(popup()).toBeNull()
  })

  it('still shows on a mouse resting there', () => {
    const weight = at('weight')
    pointer(weight, 'pointerdown', { kind: 'mouse' })
    weight.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
    vi.advanceTimersByTime(400)
    expect(popup()).toBe('In kg')
  })
})
