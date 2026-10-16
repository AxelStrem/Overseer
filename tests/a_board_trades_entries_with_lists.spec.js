import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A board that says `movable=true` trades entries with the movable lists made from its template,
// and keeps an order within its lanes. An entry dropped in a lane is moved there and takes the
// lane's value as one instruction: `move_overseer_entry` with the field to set, named from the
// entry. A board that does not say it moves nothing, and takes nothing from elsewhere.

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

const tier = (tag, colour) => base(`Tier__${tag}`, 'div', {}, [
  base('tag', 'string', { value: { String: tag } }),
  base('colour', 'string', { value: { String: colour } }),
])

const food = (name, held = '') => base(name, 'div', {}, [
  base('name', 'string', { value: { String: name } }),
  base('tier', 'enum', { value: { String: held }, vocabulary: { String: '/t/Tiers' } }),
])

const note = (name) => base(name, 'div', {}, [base('name', 'string', { value: { String: name } })])

const theDocument = ({ ranked = {} } = {}) => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Tiers', 'list', { key: { String: 'tag' }, hidden: { Boolean: true } }, [
    tier('S', '#ff7f7f'), tier('A', '#ffbf7f'), tier('B', '#ffdf7f'),
  ]),
  base('Food', 'div', { hidden: { Boolean: true } }, [
    base('name', 'string', { value: { String: '' } }),
    base('tier', 'enum', { value: { String: '' }, vocabulary: { String: '/t/Tiers' } }),
  ]),
  base('Note', 'div', { hidden: { Boolean: true } }, [base('name', 'string', { value: { String: '' } })]),
  base('Ranked', 'list', {
    entry: { Template: '<Food>' }, view: { String: 'board' }, by: { String: 'tier' },
    layout: { String: 'vertical' }, movable: { Boolean: true }, ...ranked,
  }, [food('pizza', 'S'), food('ramen', 'A'), food('tacos', 'S')]),
  base('Unranked', 'list', { entry: { Template: '<Food>' }, layout: { String: 'flow' }, movable: { Boolean: true } },
    [food('waffles'), food('donut')]),
  base('Notes', 'list', { entry: { Template: '<Note>' }, movable: { Boolean: true } }, [note('memo')]),
])]

let app

const render = (options) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(options)
  app.currentFile = 'd.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  layOut()
}

const listAt = (name) => document.querySelector(`.overseer-list[data-path='${JSON.stringify(['t', name])}']`)
const lanes = () => Array.from(listAt('Ranked').querySelectorAll('.board-column'))
const laneOf = (value) => lanes().find((c) => c.dataset.value === value)
const entryAt = (list, name) => document.querySelector(`[data-path='${JSON.stringify(['t', list, name])}']`)
const moves = () => invoke.mock.calls.filter(([cmd]) => cmd === 'move_overseer_entry').map(([, args]) => args)

/// Lanes stacked 100 pixels apart, cards 60 pixels apart across them; the plain lists below.
const layOut = () => {
  const rect = (left, top, width, height) => ({
    left, top, width, height, right: left + width, bottom: top + height, x: left, y: top,
  })
  const boxes = new Map()
  lanes().forEach((lane, i) => {
    boxes.set(lane, rect(0, i * 100, 600, 90))
    Array.from(lane.querySelectorAll(':scope > .board-column-body > [data-uid]'))
      .forEach((e, k) => boxes.set(e, rect(100 + k * 60, i * 100 + 10, 50, 50)))
  })
  ;['Unranked', 'Notes'].forEach((name, i) => {
    const list = listAt(name)
    const top = 500 + i * 100
    boxes.set(list, rect(0, top, 600, 90))
    Array.from(list.children).filter((e) => e.dataset.uid)
      .forEach((e, k) => boxes.set(e, rect(k * 60, top + 10, 50, 50)))
  })
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

/// Press on an entry, move to a point and let go there.
const dragTo = (from, x, y) => {
  const r = from.getBoundingClientRect()
  pointer(from.querySelector('.overseer-field') || from, 'pointerdown', r.left + 2, r.top + 2)
  pointer(window, 'pointermove', r.left + 2, r.top + 12)
  pointer(window, 'pointermove', x, y)
  pointer(window, 'pointerup', x, y)
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('a movable board', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    delete document.elementFromPoint
  })

  it('takes an entry from a list into a lane, among its cards, as one move that sets its value', async () => {
    render()
    app.reevaluateDocumentSelective = vi.fn(async () => {})
    // Lane S holds pizza and tacos: let go just left of the middle of tacos.
    dragTo(entryAt('Unranked', 'waffles'), 165, 30)
    await vi.waitFor(() => expect(moves()).toHaveLength(1))
    expect(moves()[0]).toMatchObject({
      entry_path: ['t', 'Unranked', 'waffles'],
      to_list_path: ['t', 'Ranked'],
      before: ['t', 'Ranked', 'tacos'],
      setting: { node_path: ['tier'], value: { String: 'S' } },
    })
    expect(moves()[0].nodePath).toBeUndefined()
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'run_overseer_event')).toBe(false)
    expect(app.reevaluateDocumentSelective).not.toHaveBeenCalled()
  })

  it('puts an entry dropped after a lane\'s last card before whatever the list has next', async () => {
    render()
    // Lane A holds ramen, and the list has tacos after it.
    dragTo(entryAt('Unranked', 'waffles'), 400, 130)
    await vi.waitFor(() => expect(moves()).toHaveLength(1))
    expect(moves()[0].before).toEqual(['t', 'Ranked', 'tacos'])
    expect(moves()[0].setting.value).toEqual({ String: 'A' })
  })

  it('puts an entry dropped in an empty lane, or after the list\'s last entry, last', async () => {
    render()
    dragTo(entryAt('Unranked', 'waffles'), 400, 230)
    dragTo(entryAt('Unranked', 'donut'), 400, 30)
    await vi.waitFor(() => expect(moves()).toHaveLength(2))
    expect(moves()[0]).toMatchObject({ before: null, setting: { value: { String: 'B' } } })
    expect(moves()[1]).toMatchObject({ before: null, setting: { value: { String: 'S' } } })
  })

  it('reorders an entry within its own lane, and writes nothing when it is let go where it was', async () => {
    render()
    dragTo(entryAt('Ranked', 'tacos'), 105, 30)
    await vi.waitFor(() => expect(moves()).toHaveLength(1))
    expect(moves()[0]).toMatchObject({
      entry_path: ['t', 'Ranked', 'tacos'], before: ['t', 'Ranked', 'pizza'],
      setting: { node_path: ['tier'], value: { String: 'S' } },
    })
    invoke.mockClear()
    dragTo(entryAt('Ranked', 'pizza'), 130, 30)
    await settle()
    expect(moves()).toHaveLength(0)
  })

  it('gives an entry to a plain list as a plain move', async () => {
    render()
    dragTo(entryAt('Ranked', 'ramen'), 300, 530)
    await vi.waitFor(() => expect(moves()).toHaveLength(1))
    expect(moves()[0].to_list_path).toEqual(['t', 'Unranked'])
    expect(moves()[0].setting).toBeUndefined()
  })

  it('takes nothing from a list made from another template', async () => {
    render()
    dragTo(entryAt('Notes', 'memo'), 400, 30)
    await settle()
    expect(moves()).toHaveLength(0)
    expect(laneOf('S').classList.contains('accepts-drop')).toBe(false)
  })

  it('takes nothing from elsewhere when it does not say movable', async () => {
    render({ ranked: { movable: { Boolean: false } } })
    dragTo(entryAt('Unranked', 'waffles'), 400, 230)
    await settle()
    expect(moves()).toHaveLength(0)
  })

  it('still only writes the value when it does not say movable', async () => {
    render({ ranked: { movable: { Boolean: false } } })
    const writes = []
    app.reevaluateDocumentSelective = vi.fn(async (paths, changes) => { writes.push(changes) })
    dragTo(entryAt('Ranked', 'ramen'), 400, 230)
    await vi.waitFor(() => expect(writes).toHaveLength(1))
    expect(writes[0]).toEqual([{ path: 't/Ranked/ramen/tier', oldValue: 'A', newValue: 'B' }])
    expect(moves()).toHaveLength(0)
  })
})
