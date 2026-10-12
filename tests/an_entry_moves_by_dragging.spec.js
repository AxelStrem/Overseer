import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// An entry of a list that says `movable=true` is dragged to another place in it, or into another
// movable list made from the same template. The page sends one instruction naming the entry, the
// list and the entry it lands before - never a position, which a filtered list does not show -
// and draws nothing until the answer says what the file holds. A press that never became a drag
// is still a press, and a finger that moves straight away is scrolling, not dragging.

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

const onClick = () => base('click', 'on', {}, [base('toggle', 'toggle', { path: { String: '../x' } })])

const card = (name, ...extra) => base(name, 'div', {}, [
  base('title', 'string', { value: { String: name } }), ...extra,
])

const list = (name, parameters, entries) => base(name, 'list', {
  entry: { Template: '<Card>' }, layout: { String: 'vertical' }, ...parameters,
}, entries)

const movable = { movable: { Boolean: true } }

const theDocument = ({ tabMutable = true } = {}) => [base('s', 'tab', { mutable: { Boolean: tabMutable } }, [
  list('A', movable, [card('a1', onClick()), card('a2'), card('a3')]),
  list('B', movable, [card('b1')]),
  list('C', {}, [card('c1')]),
  base('N', 'list', { entry: { Template: '<Note>' }, layout: { String: 'vertical' }, ...movable }, [card('n1')]),
])]

let app
let pressed

const render = (options) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(options)
  app.currentFile = 'd.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  pressed = []
  vi.spyOn(app.renderer, 'emitEvent').mockImplementation(async (node, _el, eventName) => {
    if (eventName === 'click') pressed.push(node.name)
  })
  app.renderer.renderDocument(app.currentDocument)
  layOut()
}

const listAt = (name) => document.querySelector(`.overseer-list[data-path='${JSON.stringify(['s', name])}']`)
const entryAt = (name, entry) => document.querySelector(`[data-path='${JSON.stringify(['s', name, entry])}']`)

/// The lists one under another, each entry 20 pixels high and 100 wide - jsdom lays nothing out.
const layOut = () => {
  const rect = (left, top, width, height) => ({
    left, top, width, height, right: left + width, bottom: top + height, x: left, y: top,
  })
  const boxes = new Map()
  let top = 0
  for (const name of ['A', 'B', 'C', 'N']) {
    const el = listAt(name)
    const entries = Array.from(el.children).filter((c) => c.dataset.uid)
    boxes.set(el, rect(0, top, 100, Math.max(entries.length, 1) * 20 + 10))
    entries.forEach((e, i) => boxes.set(e, rect(0, top + i * 20, 100, 20)))
    top += 200
  }
  for (const [el, box] of boxes) el.getBoundingClientRect = () => box
  // The deepest box under a point, as the browser would find it.
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

const tap = (element) => {
  const event = new window.MouseEvent('click', { bubbles: true, cancelable: true })
  element.dispatchEvent(event)
  return event
}

/// Press on an entry's title, move to a point and let go there.
const dragTo = (from, x, y) => {
  const handle = from.querySelector('.overseer-field') || from
  pointer(handle, 'pointerdown', 50, from.getBoundingClientRect().top + 5)
  pointer(window, 'pointermove', 50, from.getBoundingClientRect().top + 15)
  pointer(window, 'pointermove', x, y)
  pointer(window, 'pointerup', x, y)
}

const moves = () => invoke.mock.calls.filter(([cmd]) => cmd === 'move_overseer_entry').map(([, args]) => args)

describe('dragging an entry', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    vi.useRealTimers()
    delete document.elementFromPoint
  })

  it('marks only the lists that say movable, where writes are allowed', () => {
    render()
    expect(listAt('A').dataset.movable).toBe('1')
    expect(listAt('B').dataset.movable).toBe('1')
    expect(listAt('C').dataset.movable).toBeUndefined()
    setupDOM()
    render({ tabMutable: false })
    expect(listAt('A').dataset.movable).toBeUndefined()
  })

  it('reorders a list with one instruction naming the entry it lands before', () => {
    render()
    dragTo(entryAt('A', 'a3'), 50, 5)
    expect(moves()).toEqual([expect.objectContaining({
      path: 'd.os',
      entry_path: ['s', 'A', 'a3'],
      to_list_path: ['s', 'A'],
      before: ['s', 'A', 'a1'],
    })])
    // Each spelling once, so the door does not refuse it as a duplicate.
    expect(Object.keys(moves()[0]).filter((k) => /entry_?path/i.test(k))).toEqual(['entry_path', 'entryPath'])
  })

  it('puts an entry last when it is dropped below the rest', () => {
    render()
    dragTo(entryAt('A', 'a1'), 50, 65)
    expect(moves()[0].before).toBeNull()
  })

  it('moves an entry into another list made from its template', () => {
    render()
    dragTo(entryAt('A', 'a2'), 50, 205)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: ['s', 'A', 'a2'], to_list_path: ['s', 'B'], before: ['s', 'B', 'b1'],
    })])
  })

  it('offers no drop into a list made from another template, or one that is not movable', () => {
    render()
    dragTo(entryAt('A', 'a2'), 50, 605)
    dragTo(entryAt('A', 'a2'), 50, 405)
    expect(moves()).toEqual([])
  })

  it('names the entry drawn next when the one between is filtered out', () => {
    render()
    entryAt('A', 'a2').classList.add('filtered-out')
    dragTo(entryAt('B', 'b1'), 50, 45)
    expect(moves()[0].before).toEqual(['s', 'A', 'a3'])
  })

  it('writes nothing for a drop where the entry already is', () => {
    render()
    dragTo(entryAt('A', 'a2'), 50, 45)
    expect(moves()).toEqual([])
    expect(document.querySelector('.overseer-drag-ghost')).toBeNull()
    expect(document.querySelector('.is-dragged')).toBeNull()
  })

  it('is still a press when the pointer did not move, and not one after a drag', async () => {
    render()
    const a1 = entryAt('A', 'a1')
    pointer(a1, 'pointerdown', 50, 5)
    pointer(window, 'pointerup', 50, 5)
    tap(a1)
    expect(pressed).toEqual(['a1'])
    expect(moves()).toEqual([])

    dragTo(a1, 50, 65)
    tap(a1)
    expect(pressed).toEqual(['a1'])
    await new Promise((r) => setTimeout(r, 0))
    tap(a1)
    expect(pressed).toEqual(['a1', 'a1'])
  })

  it('takes a finger that moves at once for a scroll, and one held still for a drag', () => {
    vi.useFakeTimers()
    render()
    const a1 = entryAt('A', 'a1')
    pointer(a1, 'pointerdown', 50, 5, 'touch')
    pointer(window, 'pointermove', 50, 45, 'touch')
    vi.advanceTimersByTime(400)
    pointer(window, 'pointerup', 50, 65, 'touch')
    expect(moves()).toEqual([])

    pointer(a1, 'pointerdown', 50, 5, 'touch')
    vi.advanceTimersByTime(400)
    pointer(window, 'pointermove', 50, 65, 'touch')
    pointer(window, 'pointerup', 50, 65, 'touch')
    expect(moves()).toEqual([expect.objectContaining({ entry_path: ['s', 'A', 'a1'], before: null })])
  })

  it('lets Escape put the entry down where it was', () => {
    render()
    const a3 = entryAt('A', 'a3')
    pointer(a3, 'pointerdown', 50, 45)
    pointer(window, 'pointermove', 50, 30)
    pointer(window, 'pointermove', 50, 5)
    window.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape' }))
    pointer(window, 'pointerup', 50, 5)
    expect(moves()).toEqual([])
    expect(document.querySelector('.overseer-drop-line')).toBeNull()
  })
})
