import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A list drawn in an order of its own (`sort_by`) that also says `movable=true` can be reordered
// by hand among the entries its order ties: those are drawn in the order the file has them, and a
// drag writes that order and nothing else. Pinned items are the case it was made for - the list
// sorts them first, they say `movable=false`, and the rest drag among themselves below them.

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

/// An entry as the resolver hands it over, with the key its list's `sort_by` worked out for it.
const card = (name, key, more = {}) => base(name, 'div', { _ui_sort_key: { Integer: key }, ...more }, [
  base('title', 'string', { value: { String: name } }),
])
const pinned = (name) => card(name, 0, { movable: { Boolean: false } })

const list = (name, template, entries) => base(name, 'list', {
  entry: { Template: template }, layout: { String: 'vertical' }, movable: { Boolean: true },
  sort_by: { String: '|x| x/pinned ? 0 : 1' },
}, entries)

const theDocument = () => [base('s', 'tab', { mutable: { Boolean: true } }, [
  list('A', '<Card>', [card('u1', 1), card('u2', 1), pinned('p1'), card('u3', 1)]),
  list('B', '<Other>', [card('b1', 1), card('b2', 2)]),
  list('C', '<Q>', [card('q1', 1), card('q2', 1), pinned('qp')]),
])]

let app

const render = () => {
  app = new OverseerApp()
  window.app = app
  app.currentDocument = theDocument()
  app.currentFile = 'd.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  layOut()
}

const listAt = (name) => document.querySelector(`.overseer-list[data-path='${JSON.stringify(['s', name])}']`)
const entryAt = (name, entry) => document.querySelector(`[data-path='${JSON.stringify(['s', name, entry])}']`)
const drawn = (name) => Array.from(listAt(name).children).filter((c) => c.dataset.uid)
  .map((c) => JSON.parse(c.dataset.path).slice(-1)[0])

/// The lists one under another, each entry 20 pixels high and 100 wide, in the order they are
/// drawn - jsdom lays nothing out.
const layOut = () => {
  const rect = (left, top, width, height) => ({
    left, top, width, height, right: left + width, bottom: top + height, x: left, y: top,
  })
  const boxes = new Map()
  let top = 0
  for (const name of ['A', 'B', 'C']) {
    const el = listAt(name)
    const entries = Array.from(el.children).filter((c) => c.dataset.uid)
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

/// Press on an entry's title, move to a point and let go there.
const dragTo = (from, x, y) => {
  const handle = from.querySelector('.overseer-field') || from
  pointer(handle, 'pointerdown', 50, from.getBoundingClientRect().top + 5)
  pointer(window, 'pointermove', 50, from.getBoundingClientRect().top + 15)
  pointer(window, 'pointermove', x, y)
  pointer(window, 'pointerup', x, y)
}

const moves = () => invoke.mock.calls.filter(([cmd]) => cmd === 'move_overseer_entry').map(([, args]) => args)

describe('an entry moves among the entries its order ties', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    delete document.elementFromPoint
  })

  it("draws the entries in the list's order, ties in the order of the file", () => {
    render()
    expect(drawn('A')).toEqual(['p1', 'u1', 'u2', 'u3'])
    expect(drawn('C')).toEqual(['qp', 'q1', 'q2'])
  })

  it('lands before the tie it is dropped before', () => {
    render()
    dragTo(entryAt('A', 'u3'), 50, 25)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: ['s', 'A', 'u3'], to_list_path: ['s', 'A'], before: ['s', 'A', 'u1'],
    })])
  })

  it('goes no higher than the first of its ties, however high it is dropped', () => {
    render()
    dragTo(entryAt('A', 'u3'), 50, 5)
    expect(moves()[0].before).toEqual(['s', 'A', 'u1'])
  })

  it('writes nothing when dropped above its ties while it is the first of them', () => {
    render()
    dragTo(entryAt('A', 'u1'), 50, 5)
    expect(moves()).toEqual([])
  })

  it('goes last of its ties when dropped below them, and last in the file when they end it', () => {
    render()
    dragTo(entryAt('A', 'u1'), 50, 75)
    expect(moves()[0].before).toBeNull()
  })

  it('goes before what the file has after the last of its ties, when that is not one', () => {
    render()
    dragTo(entryAt('C', 'q1'), 50, 455)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: ['s', 'C', 'q1'], to_list_path: ['s', 'C'], before: ['s', 'C', 'qp'],
    })])
  })

  it('leaves an entry that says it stays where it is', () => {
    render()
    dragTo(entryAt('A', 'p1'), 50, 75)
    expect(moves()).toEqual([])
  })

  it('cannot be dragged when its order ties it with nothing', () => {
    render()
    dragTo(entryAt('B', 'b2'), 50, 205)
    expect(moves()).toEqual([])
  })
})
