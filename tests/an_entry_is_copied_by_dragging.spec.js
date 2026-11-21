import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// An entry of a list that says `copyable=true` is copied by dragging it with the copy key held -
// Ctrl, or Option on a Mac - and by any drag where its list copies but does not move. The page
// sends one instruction naming the entry, the list and the entry the copy lands before, and offers
// no list that would refuse it: another template, one that says neither `movable` nor `copyable`,
// or a keyed list that holds the entry's key already, its own among them. A finger has no key, so
// a list that only copies is not dragged by one.

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

const card = (name) => base(name, 'div', {}, [base('title', 'string', { value: { String: name } })])

const list = (name, parameters, entries) => base(name, 'list', {
  entry: { Template: '<Card>' }, layout: { String: 'vertical' }, ...parameters,
}, entries)

const yes = { Boolean: true }
const keyed = { key: { String: 'title' } }

const theDocument = () => [base('s', 'tab', { mutable: yes }, [
  list('P', { copyable: yes, ...keyed }, [card('p1'), card('p2')]),
  list('A', { movable: yes, copyable: yes }, [card('a1'), card('a2')]),
  list('B', { movable: yes, ...keyed }, [card('p1')]),
  list('C', {}, [card('c1')]),
])]

let app

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app.currentFile = 'd.os'
  app._currentText = 'TEXT'
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
  for (const name of ['P', 'A', 'B', 'C']) {
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

const pointer = (target, type, x, y, { pointerType = 'mouse', ...keys } = {}) => {
  const event = new window.MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, ...keys })
  Object.defineProperty(event, 'pointerId', { value: 1 })
  Object.defineProperty(event, 'pointerType', { value: pointerType })
  target.dispatchEvent(event)
  return event
}

/// Press on an entry's title, move to a point and let go there, the keys said held throughout.
const dragTo = (from, x, y, keys = {}) => {
  const handle = from.querySelector('.overseer-field') || from
  pointer(handle, 'pointerdown', 50, from.getBoundingClientRect().top + 5, keys)
  pointer(window, 'pointermove', 50, from.getBoundingClientRect().top + 15, keys)
  pointer(window, 'pointermove', x, y, keys)
  pointer(window, 'pointerup', x, y, keys)
}

const sent = (command) => invoke.mock.calls.filter(([cmd]) => cmd === command).map(([, args]) => args)
const copies = () => sent('copy_overseer_entry')
const moves = () => sent('move_overseer_entry')

describe('copying an entry by dragging', () => {
  const platform = Object.getOwnPropertyDescriptor(window.navigator, 'platform')
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    delete document.elementFromPoint
    if (platform) Object.defineProperty(window.navigator, 'platform', platform)
    else delete window.navigator.platform
  })

  it('marks the lists that say copyable apart from the ones that move', () => {
    render()
    expect(listAt('P').dataset.copyable).toBe('1')
    expect(listAt('P').dataset.movable).toBeUndefined()
    expect(listAt('A').dataset.copyable).toBe('1')
    expect(listAt('A').dataset.movable).toBe('1')
    expect(listAt('B').dataset.copyable).toBeUndefined()
    expect(listAt('C').dataset.copyable).toBeUndefined()
  })

  it('copies out of a list that only copies with a plain drag, before the entry it lands on', () => {
    render()
    dragTo(entryAt('P', 'p2'), 50, 205)
    expect(moves()).toEqual([])
    expect(copies()).toEqual([expect.objectContaining({
      path: 'd.os', entry_path: ['s', 'P', 'p2'], to_list_path: ['s', 'A'], before: ['s', 'A', 'a1'],
    })])
    expect(Object.keys(copies()[0]).filter((k) => /entry_?path/i.test(k))).toEqual(['entry_path', 'entryPath'])
  })

  it('offers no keyed list that holds the key already, its own among them', () => {
    render()
    dragTo(entryAt('P', 'p1'), 50, 45)
    dragTo(entryAt('P', 'p1'), 50, 405)
    expect(copies()).toEqual([])
    dragTo(entryAt('P', 'p2'), 50, 405)
    expect(copies()).toEqual([expect.objectContaining({ entry_path: ['s', 'P', 'p2'], to_list_path: ['s', 'B'] })])
  })

  it('offers no list that says neither movable nor copyable', () => {
    render()
    dragTo(entryAt('P', 'p2'), 50, 605)
    expect(copies()).toEqual([])
  })

  it('moves with a plain drag and copies with Ctrl held where a list does both', () => {
    render()
    dragTo(entryAt('A', 'a2'), 50, 205)
    expect(moves()).toEqual([expect.objectContaining({ entry_path: ['s', 'A', 'a2'], before: ['s', 'A', 'a1'] })])
    expect(copies()).toEqual([])
    dragTo(entryAt('A', 'a2'), 50, 245, { ctrlKey: true })
    expect(copies()).toEqual([expect.objectContaining({
      entry_path: ['s', 'A', 'a2'], to_list_path: ['s', 'A'], before: null,
    })])
  })

  it('reads the key as the entry is let go, and shows a copy while it is held', () => {
    render()
    const a1 = entryAt('A', 'a1')
    pointer(a1.querySelector('.overseer-field') || a1, 'pointerdown', 50, 205)
    pointer(window, 'pointermove', 50, 215)
    expect(document.querySelector('.overseer-drag-shell.is-copy')).toBeNull()
    window.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Control', ctrlKey: true }))
    expect(document.querySelector('.overseer-drag-shell.is-copy')).not.toBeNull()
    expect(document.body.classList.contains('overseer-copying')).toBe(true)
    // Its own list is a place to copy into, since it holds no keys.
    pointer(window, 'pointermove', 50, 245, { ctrlKey: true })
    pointer(window, 'pointerup', 50, 245)
    expect(copies()).toEqual([])
    expect(moves()).toEqual([expect.objectContaining({ entry_path: ['s', 'A', 'a1'], before: null })])
    expect(document.body.classList.contains('overseer-copying')).toBe(false)
  })

  it('takes Option for the key on a Mac, where Ctrl and a click is the right button', () => {
    Object.defineProperty(window.navigator, 'platform', { value: 'MacIntel', configurable: true })
    render()
    dragTo(entryAt('A', 'a2'), 50, 205, { ctrlKey: true })
    expect(copies()).toEqual([])
    dragTo(entryAt('A', 'a2'), 50, 205, { altKey: true })
    expect(copies()).toEqual([expect.objectContaining({ entry_path: ['s', 'A', 'a2'] })])
  })

  it('is not dragged by a finger in a list that only copies', () => {
    vi.useFakeTimers()
    try {
      render()
      const p2 = entryAt('P', 'p2')
      const touch = { pointerType: 'touch' }
      pointer(p2.querySelector('.overseer-field') || p2, 'pointerdown', 50, 25, touch)
      vi.advanceTimersByTime(400)
      pointer(window, 'pointermove', 50, 205, touch)
      pointer(window, 'pointerup', 50, 205, touch)
      expect(copies()).toEqual([])
      expect(moves()).toEqual([])
    } finally {
      vi.useRealTimers()
    }
  })
})
