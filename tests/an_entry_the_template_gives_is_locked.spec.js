import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// An entry its list's template gives is the template's: in a document that inherits it, it is
// neither dragged nor edited. Its grip is faint and lifts nothing, and its fields read as
// `mutable=false`, while the entries the document wrote itself beside it move and edit as before.

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

const card = (name, parameters = {}) => base(name, 'div', parameters, [
  wrapper('div', 'div', { layout: { String: 'horizontal' } }, [
    grip(),
    base('title', 'string', { value: { String: name } }),
  ]),
])

const list = (name, parameters, entries) => base(name, 'list', {
  entry: { Template: '<Card>' }, layout: { String: 'vertical' }, ...parameters,
}, entries)

const movable = { movable: { Boolean: true } }
const table = {
  view: { String: 'table' },
  _columns: { String: JSON.stringify([{ name: 'title', label: 'title', width: '30%', span: false }]) },
}

const theDocument = () => [base('s', 'tab', { mutable: { Boolean: true } }, [
  list('A', movable, [card('a1'), card('a2'), card('a3')]),
  list('T', { ...movable, ...table }, [card('t1'), card('t2'), card('t3')]),
  list('C', {}, [card('c1'), card('c2')]),
])]

let app

const render = (made = theDocument()) => {
  app = new OverseerApp()
  app.currentDocument = made
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

const pinned = { movable: { Formula: 'pinned == false' }, _computed_movable: { Boolean: false } }

const moves = () => invoke.mock.calls.filter(([cmd]) => cmd === 'move_overseer_entry').map(([, args]) => args)

const given = { _entry_from_template: { Boolean: true } }

describe('an entry the template gives', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    vi.useRealTimers()
    delete document.elementFromPoint
  })

  const theInstance = () => [base('s', 'tab', { mutable: { Boolean: true } }, [
    list('A', movable, [card('a1', given), card('a2'), card('a3')]),
    list('T', { ...movable, ...table }, []),
    list('C', {}, []),
  ])]

  it('lifts nothing, and its grip is faint, where the entries the document wrote drag', () => {
    render(theInstance())
    const a1 = entryAt('A', 'a1')
    expect(gripOf(a1).dataset.live).toBe('0')
    expect(gripOf(entryAt('A', 'a2')).dataset.live).toBe('1')
    pointer(gripOf(a1), 'pointerdown', 50, 5)
    pointer(window, 'pointermove', 50, 30)
    pointer(window, 'pointermove', 50, 45)
    pointer(window, 'pointerup', 50, 45)
    expect(document.querySelector('.overseer-drag-ghost')).toBeNull()
    expect(moves()).toEqual([])
  })

  it('is not offered for editing, where the entries the document wrote are', () => {
    render(theInstance())
    const doc = app.currentDocument
    expect(app.renderer._computeEffectiveMutableAtPath(doc, ['s', 'A', 'a1', 'title'])).toBe('false')
    expect(app.renderer._computeEffectiveMutableAtPath(doc, ['s', 'A', 'a2', 'title'])).toBe('true')
  })
})
