import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A tree's row is dragged with its branch. The gap between rows it is let go in says where it
// goes, and how far right it was carried says how deep, one depth for each `indent`, as in an
// outliner - with `movable="anywhere"`; with `movable=true` it stays among its siblings. A tree
// by a parent field writes the parent with the move; one that holds its children nested moves
// the entry into the parent's own list.

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
const text = (name, value) => base(name, 'string', { value: { String: value } })

// Side by side, in the order written: site > pages > about, site > deploy, launch.
const task = ([handle, parent]) => base(handle, 'div', {}, [text('handle', handle), text('parent', parent)])
const flat = (movable) => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Tasks', 'list', {
    entry: { Template: '<Task>' }, key: { String: 'handle' }, tree: { String: 'parent' }, movable,
  }, [['site', ''], ['pages', 'site'], ['about', 'pages'], ['deploy', 'site'], ['launch', '']].map(task)),
])]

// Nested: Car > Engine > Pistons, Car > Wheels, Bicycle.
const part = (name, kids = []) => base(name, 'div', {}, [
  text('name', name),
  base('Parts', 'list', { entry: { Template: '<Part>' }, key: { String: 'name' } }, kids.map(([k, sub]) => part(k, sub))),
])
const nested = (movable) => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Parts', 'list', {
    entry: { Template: '<Part>' }, key: { String: 'name' }, tree: { String: 'Parts' }, movable,
  }, [part('Car', [['Engine', [['Pistons', []]]], ['Wheels', []]]), part('Bicycle')]),
])]

// Kept in an order of its own, pinned first: pin, then site > pages, and launch.
const keyed = ([handle, parent, key]) => base(handle, 'div', { _ui_sort_key: { Integer: key } },
  [text('handle', handle), text('parent', parent)])
const sorted = () => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Tasks', 'list', {
    entry: { Template: '<Task>' }, key: { String: 'handle' }, tree: { String: 'parent' },
    movable: { String: 'anywhere' }, sort_by: { String: '|x| x/pinned' },
  }, [['pin', '', 0], ['site', '', 1], ['pages', 'site', 1], ['launch', '', 1]].map(keyed)),
])]

const anywhere = { String: 'anywhere' }
const siblings = { Boolean: true }

let app
const open = (doc) => {
  app = new OverseerApp()
  app.currentDocument = doc
  app.currentFile = 'd.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  layOut()
}

const list = () => document.querySelector('.overseer-list.view-tree')
const rows = () => Array.from(list().querySelectorAll(':scope > .tree-row'))
const nameOf = (row) => JSON.parse(row.dataset.path).at(-1)
const rowNamed = (name) => rows().find((row) => nameOf(row) === name)

/// Rows 20 pixels high, one under another, each standing in 18 pixels a depth - jsdom lays
/// nothing out.
const layOut = () => {
  const rect = (left, top, width, height) => ({
    left, top, width, height, right: left + width, bottom: top + height, x: left, y: top,
  })
  const boxes = new Map()
  boxes.set(list(), rect(0, 0, 400, rows().length * 20))
  rows().forEach((row, i) => {
    const left = (Number(row.dataset.treeDepth) + 1) * 18
    boxes.set(row, rect(left, i * 20, 400 - left, 20))
  })
  for (const [el, box] of boxes) el.getBoundingClientRect = () => box
  document.elementFromPoint = (x, y) => {
    let found = null
    for (const [el, box] of boxes) {
      if (x >= box.left && x < box.right && y >= box.top && y < box.bottom) {
        if (!found || found.contains(el)) found = el
      }
    }
    return found || list()
  }
}

const pointer = (target, type, x, y) => {
  const event = new window.MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 })
  Object.defineProperty(event, 'pointerId', { value: 1 })
  Object.defineProperty(event, 'pointerType', { value: 'mouse' })
  target.dispatchEvent(event)
}

/// Press on a row, carry it `right` pixels right, and let go at height `y`.
const drag = (name, y, right = 0) => {
  const row = rowNamed(name)
  const r = row.getBoundingClientRect()
  const x = r.left + 100
  pointer(row, 'pointerdown', x, r.top + 5)
  pointer(window, 'pointermove', x, r.top + 15)
  pointer(window, 'pointermove', x + right, y)
  pointer(window, 'pointerup', x + right, y)
}

const moves = () => invoke.mock.calls.filter(([cmd]) => cmd === 'move_overseer_entry').map(([, args]) => args)
const at = (...names) => ['t', ...names]

describe('a tree entry moves with its branch', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    delete document.elementFromPoint
  })

  it('drags a nested tree, which was not offered before', () => {
    open(nested(anywhere))
    expect(list().dataset.movable).toBe('1')
    expect(list().dataset.treeMove).toBe('anywhere')
  })

  it('moves a nested entry under another, carried right', () => {
    open(nested(anywhere))
    // Wheels (depth 1) let go after Pistons (depth 2): one depth right is Pistons' sibling,
    // two is its child.
    drag('Wheels', 58, 18)
    drag('Wheels', 58, 36)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: at('Parts', 'Car', 'Parts', 'Wheels'),
      to_list_path: at('Parts', 'Car', 'Parts', 'Engine', 'Parts'),
      before: null,
    }), expect.objectContaining({
      to_list_path: at('Parts', 'Car', 'Parts', 'Engine', 'Parts', 'Pistons', 'Parts'),
      before: null,
    })])
  })

  it('moves a nested entry to the top, carried left', () => {
    open(nested(anywhere))
    // Engine's branch lifted; dropped below Wheels, two depths left: after Car, before Bicycle.
    drag('Engine', 78, -40)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: at('Parts', 'Car', 'Parts', 'Engine'),
      to_list_path: at('Parts'),
      before: at('Parts', 'Bicycle'),
    })])
  })

  it('keeps an entry among its siblings when the tree says movable=true', () => {
    open(nested(siblings))
    // Wheels before Engine: the same list.
    drag('Wheels', 22)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: at('Parts', 'Car', 'Parts', 'Wheels'),
      to_list_path: at('Parts', 'Car', 'Parts'),
      before: at('Parts', 'Car', 'Parts', 'Engine'),
    })])
    invoke.mockClear()
    // Carried right under Pistons, or dropped among Car's own siblings: nowhere it may go.
    drag('Wheels', 58, 18)
    drag('Wheels', 98)
    expect(moves()).toEqual([])
  })

  it('cannot drop an entry inside its own branch', () => {
    open(nested(anywhere))
    // Car dropped between its own rows: those rows go with it, so the gap is after the branch.
    drag('Car', 42, 36)
    for (const move of moves()) {
      expect(move.to_list_path.slice(0, 3)).not.toEqual(at('Parts', 'Car'))
    }
  })

  it('writes nothing for a drop where the row already is', () => {
    open(flat(anywhere))
    drag('deploy', 66)
    expect(moves()).toEqual([])
    expect(document.querySelector('.is-dragged')).toBeNull()
  })

  it('gives a flat entry its new parent with the move', () => {
    open(flat(anywhere))
    // Rows: site, pages, about, deploy, launch. launch carried under deploy, one depth in.
    drag('launch', 79, 36)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: at('Tasks', 'launch'),
      to_list_path: at('Tasks'),
      before: null,
      setting: { node_path: ['parent'], value: { String: 'deploy' } },
    })])
  })

  it('puts a flat entry before its new next sibling, and to the top with no parent', () => {
    open(flat(anywhere))
    // pages (with about) to the top, before launch.
    drag('pages', 82, -18)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: at('Tasks', 'pages'),
      before: at('Tasks', 'launch'),
      setting: { node_path: ['parent'], value: { String: '' } },
    })])
  })

  it('puts a flat entry after its last sibling in the file when it ends its parent', () => {
    open(flat(anywhere))
    // about out of pages, to the end of site's children: after deploy in the file.
    drag('about', 79, -18)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: at('Tasks', 'about'),
      before: at('Tasks', 'launch'),
      setting: { node_path: ['parent'], value: { String: 'site' } },
    })])
  })

  it('keeps a flat entry under its parent when the tree says movable=true', () => {
    open(flat(siblings))
    drag('deploy', 22)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: at('Tasks', 'deploy'),
      before: at('Tasks', 'pages'),
      setting: { node_path: ['parent'], value: { String: 'site' } },
    })])
    invoke.mockClear()
    drag('deploy', 98, -18)
    expect(moves()).toEqual([])
  })

  it('moves a row of a tree kept sorted under another parent, among the rows it ties with', () => {
    open(sorted())
    // Rows: pin, site, pages, launch. launch carried in under site, after pages.
    drag('launch', 58, 18)
    expect(moves()).toEqual([expect.objectContaining({
      entry_path: at('Tasks', 'launch'),
      before: null,
      setting: { node_path: ['parent'], value: { String: 'site' } },
    })])
  })

  it('does not drop a row of a tree kept sorted where its order would not put it', () => {
    open(sorted())
    // Above pin, which sorts before everything else.
    drag('launch', 2)
    drag('pages', 2, -18)
    expect(moves()).toEqual([])
  })
})
