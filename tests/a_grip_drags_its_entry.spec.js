import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// `mark (type="grip")` in an entry is what the entry is always dragged by: a press on it lifts the
// entry even where the press would otherwise select a value's text, and a finger on it drags at
// once, with no hold first. Where the list cannot be dragged, the grip lifts nothing.

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

const wrapper = (name, node_type, parameters, children) => ({ ...base(name, node_type, parameters, children), is_hierarchy_transparent: true })

const grip = () => wrapper('mark', 'mark', { type: { String: 'grip' } })

const card = (name) => base(name, 'div', {}, [
  wrapper('div', 'div', { layout: { String: 'horizontal' } }, [
    grip(),
    base('title', 'string', { value: { String: name } }),
  ]),
])

const list = (name, parameters, entries) => base(name, 'list', {
  entry: { Template: '<Card>' }, layout: { String: 'vertical' }, ...parameters,
}, entries)

const movable = { movable: { Boolean: true } }
const table = { view: { String: 'table' } }

const theDocument = () => [base('s', 'tab', { mutable: { Boolean: true } }, [
  list('A', movable, [card('a1'), card('a2'), card('a3')]),
  list('T', { ...movable, ...table }, [card('t1'), card('t2'), card('t3')]),
  list('C', {}, [card('c1'), card('c2')]),
])]

let app

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app.currentFile = 'd.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  layOut()
}

const listAt = (name) => document.querySelector(`.overseer-list[data-path='${JSON.stringify(['s', name])}']`)
const entryAt = (name, entry) => document.querySelector(`[data-path='${JSON.stringify(['s', name, entry])}']`)
const gripOf = (entry) => entry.querySelector('.mark-grip')

/// The lists one under another, each entry 20 pixels high and 100 wide - jsdom lays nothing out.
const layOut = () => {
  const rect = (left, top, width, height) => ({
    left, top, width, height, right: left + width, bottom: top + height, x: left, y: top,
  })
  const boxes = new Map()
  let top = 0
  for (const name of ['A', 'T', 'C']) {
    const el = listAt(name)
    const entries = Array.from(el.querySelectorAll('[data-uid]')).filter((c) => app.renderer.isEntryElementOf(c, el))
    boxes.set(el, rect(0, top, 100, Math.max(entries.length, 1) * 20 + 10))
    entries.forEach((e, i) => boxes.set(e, rect(0, top + i * 20, 100, 20)))
    top += 200
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

const pointer = (target, type, x, y, pointerType = 'mouse') => {
  const event = new window.MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 })
  Object.defineProperty(event, 'pointerId', { value: 1 })
  Object.defineProperty(event, 'pointerType', { value: pointerType })
  target.dispatchEvent(event)
  return event
}

/// Press on a target in the third entry of a list at `top`, move up to the first and let go.
const pressAndMove = (target, top, pointerType = 'mouse') => {
  pointer(target, 'pointerdown', 50, top + 45, pointerType)
  pointer(window, 'pointermove', 50, top + 30, pointerType)
  pointer(window, 'pointermove', 50, top + 5, pointerType)
  pointer(window, 'pointerup', 50, top + 5, pointerType)
}

const moves = () => invoke.mock.calls.filter(([cmd]) => cmd === 'move_overseer_entry').map(([, args]) => args)

describe('a grip', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    vi.useRealTimers()
    delete document.elementFromPoint
  })

  it('is drawn as a grip icon, and a mark of a type the page does not know as nothing', () => {
    render()
    const drawn = gripOf(entryAt('A', 'a1'))
    expect(drawn).not.toBeNull()
    expect(drawn.querySelector('.icon')).not.toBeNull()
    expect(drawn.closest('.overseer-field')).toBeNull()

    const unknown = app.renderer.createMarkElement(base('mark', 'mark', { type: { String: 'sparkle' } }))
    expect(unknown.children).toHaveLength(0)
    expect(unknown.textContent).toBe('')
  })

  it('drags its entry, where a press on the value beside it does not', () => {
    render()
    const a3 = entryAt('A', 'a3')
    const value = a3.querySelector('.overseer-field .field-value, .overseer-field .text-content')
    expect(value).not.toBeNull()
    pressAndMove(value, 0)
    expect(moves()).toEqual([])

    pressAndMove(gripOf(a3).querySelector('.icon'), 0)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: ['s', 'A', 'a3'], to_list_path: ['s', 'A'], before: ['s', 'A', 'a1'],
    })])
  })

  it('drags a table row', () => {
    render()
    const t3 = entryAt('T', 't3')
    expect(gripOf(t3)).not.toBeNull()
    pressAndMove(gripOf(t3), 200)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: ['s', 'T', 't3'], before: ['s', 'T', 't1'],
    })])
  })

  it('drags at once under a finger, where elsewhere a finger that moves is scrolling', () => {
    vi.useFakeTimers()
    render()
    const a3 = entryAt('A', 'a3')
    pressAndMove(a3, 0, 'touch')
    expect(moves()).toEqual([])

    pressAndMove(gripOf(a3), 0, 'touch')
    expect(moves()).toEqual([expect.objectContaining({ entry_path: ['s', 'A', 'a3'], before: ['s', 'A', 'a1'] })])
  })

  it('lifts nothing in a list that is not movable', () => {
    render()
    expect(listAt('C').dataset.movable).toBeUndefined()
    const c2 = entryAt('C', 'c2')
    pointer(gripOf(c2), 'pointerdown', 50, 425)
    pointer(window, 'pointermove', 50, 410)
    pointer(window, 'pointermove', 50, 405)
    pointer(window, 'pointerup', 50, 405)
    expect(document.querySelector('.overseer-drag-ghost')).toBeNull()
    expect(moves()).toEqual([])
  })
})
