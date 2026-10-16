import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A list that says `view="board", by="stage"` is drawn in columns, one for each value its entries'
// `enum stage` can hold, in the order the field's vocabulary lists them. Dropping an entry in
// another column writes the field, as picking the value from its chip does; nothing moves in the
// list itself. A value written any other way puts the entry in its new column too.

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

const stage = (tag, colour) => base(`Stage__${tag}`, 'div', {}, [
  base('tag', 'string', { value: { String: tag } }),
  base('colour', 'string', { value: { String: colour } }),
])

const stageField = (held, extra = {}, children = []) => base('stage', 'enum', {
  value: { String: held }, vocabulary: { String: '/s/Stages' }, ...extra,
}, children)

const card = (name, held, extra, children) => base(name, 'div', {}, [
  base('title', 'string', { value: { String: name } }), stageField(held, extra, children),
])

const theDocument = ({ entries, mutable = true, board = {} } = {}) => [base('s', 'tab', { mutable: { Boolean: mutable } }, [
  base('Stages', 'list', { key: { String: 'tag' }, hidden: { Boolean: true } }, [
    stage('filed', '#6b7280'), stage('ready', '#1d4ed8'), stage('testing', '#a16207'),
  ]),
  base('Card', 'div', { hidden: { Boolean: true } }, [
    base('title', 'string', { value: { String: '' } }), stageField('filed'),
  ]),
  base('Items', 'list', {
    entry: { Template: '<Card>' },
    view: { String: 'board' }, by: { String: 'stage' }, ...board,
  }, entries ?? [card('one', 'ready'), card('two', 'filed'), card('three', 'ready'), card('four', 'shipped')]),
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

const board = () => document.querySelector(`.overseer-list[data-path='${JSON.stringify(['s', 'Items'])}']`)
const columns = () => Array.from(board().querySelectorAll('.board-column'))
const columnOf = (value) => columns().find((c) => c.dataset.value === value)
const titlesIn = (value) => Array.from(columnOf(value).querySelectorAll(':scope > .board-column-body > [data-uid]'))
  .map((el) => JSON.parse(el.dataset.path).pop())
const entryAt = (name) => document.querySelector(`[data-path='${JSON.stringify(['s', 'Items', name])}']`)
const stageOf = (name) => app.currentDocument[0].children[2].children
  .find((c) => c.name === name).children[1].parameters.value

/// The columns side by side, 200 pixels apart, each entry 20 pixels high - jsdom lays nothing out.
const layOut = () => {
  const rect = (left, top, width, height) => ({
    left, top, width, height, right: left + width, bottom: top + height, x: left, y: top,
  })
  const boxes = new Map()
  columns().forEach((column, i) => {
    boxes.set(column, rect(i * 200, 0, 150, 300))
    Array.from(column.querySelectorAll(':scope > .board-column-body > [data-uid]'))
      .forEach((e, k) => boxes.set(e, rect(i * 200, 30 + k * 20, 150, 20)))
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

/// Press on an entry's title, move into a column and let go there.
const dragTo = (from, value) => {
  const r = from.getBoundingClientRect()
  const to = columnOf(value).getBoundingClientRect()
  pointer(from.querySelector('.overseer-field') || from, 'pointerdown', r.left + 5, r.top + 5)
  pointer(window, 'pointermove', r.left + 5, r.top + 15)
  pointer(window, 'pointermove', to.left + 10, 250)
  pointer(window, 'pointerup', to.left + 10, 250)
}

describe('a list drawn as a board', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockClear()
  })
  afterEach(() => {
    delete document.elementFromPoint
  })

  it('draws a column for each value, in the order the vocabulary lists them', () => {
    render()
    expect(columns().map((c) => c.dataset.value)).toEqual(['filed', 'ready', 'testing', ''])
    expect(columnOf('ready').querySelector('.tag-chip').style.backgroundColor).toBe('rgb(29, 78, 216)')
  })

  it('puts each entry in the column of the value it holds, in the order of the list', () => {
    render()
    expect(titlesIn('filed')).toEqual(['two'])
    expect(titlesIn('ready')).toEqual(['one', 'three'])
    expect(titlesIn('testing')).toEqual([])
    expect(columnOf('ready').querySelector('.board-column-count').textContent).toBe('2')
  })

  it('shows a value the vocabulary does not list in a column of its own', () => {
    render()
    expect(columnOf('').dataset.unlisted).toBe('1')
    expect(titlesIn('')).toEqual(['four'])
  })

  it('draws its columns from the template when it has no entries', () => {
    render({ entries: [] })
    expect(columns().map((c) => c.dataset.value)).toEqual(['filed', 'ready', 'testing'])
  })

  it('is an ordinary list when the field it names is not an enum', () => {
    render({ board: { by: { String: 'title' } } })
    expect(board().querySelector('.overseer-board')).toBeNull()
    expect(board().classList.contains('view-board')).toBe(false)
  })

  it('writes the value of the column an entry is dropped in, and moves it there', async () => {
    render()
    const writes = []
    app.reevaluateDocumentSelective = vi.fn(async (paths, changes) => { writes.push(changes) })
    dragTo(entryAt('two'), 'testing')
    await vi.waitFor(() => expect(writes).toHaveLength(1))

    expect(writes[0]).toEqual([{ path: 's/Items/two/stage', oldValue: 'filed', newValue: 'testing' }])
    expect(stageOf('two')).toEqual({ String: 'testing' })
    expect(titlesIn('testing')).toEqual(['two'])
    expect(titlesIn('filed')).toEqual([])
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'move_overseer_entry')).toBe(false)
  })

  it('sends the value with its change handler, as one instruction', async () => {
    const handler = [base('change', 'on', {}, [base('set', 'set')])]
    render({ entries: [card('one', 'ready', {}, handler), card('two', 'filed')] })
    app.reevaluateDocumentSelective = vi.fn(async () => {})
    dragTo(entryAt('one'), 'testing')
    await vi.waitFor(() => expect(invoke.mock.calls.some(([cmd]) => cmd === 'run_overseer_event')).toBe(true))

    const [, args] = invoke.mock.calls.find(([cmd]) => cmd === 'run_overseer_event')
    expect(args.event_name).toBe('change')
    expect(args.writing).toEqual([{ node_path: ['s', 'Items', 'one', 'stage'], value: { String: 'testing' } }])
    expect(app.reevaluateDocumentSelective).not.toHaveBeenCalled()
  })

  it('takes no drop in its own column, in a value held back, or among the values not listed', async () => {
    render({ entries: [card('one', 'ready', { withhold: { String: 'testing' } }), card('two', 'shipped')] })
    app.reevaluateDocumentSelective = vi.fn(async () => {})
    dragTo(entryAt('one'), 'ready')
    dragTo(entryAt('one'), 'testing')
    dragTo(entryAt('one'), '')
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(app.reevaluateDocumentSelective).not.toHaveBeenCalled()
    expect(stageOf('one')).toEqual({ String: 'ready' })
  })

  it('offers no drag where the document cannot be changed', () => {
    render({ mutable: false })
    expect(board().dataset.boardDrag).toBeUndefined()
  })

  it('runs its lanes side by side unless it says otherwise', () => {
    render()
    expect(board().querySelector('.overseer-board').classList.contains('board-columns')).toBe(true)
    expect(columnOf('ready').querySelector('.board-column-head').style.backgroundColor).toBe('')
  })

  it('stacks its lanes as rows when it says layout vertical, each headed in its colour', () => {
    render({ board: { layout: { String: 'vertical' } } })
    const drawn = board().querySelector('.overseer-board')
    expect(drawn.classList.contains('board-rows')).toBe(true)
    expect(columns().map((c) => c.dataset.value)).toEqual(['filed', 'ready', 'testing', ''])
    expect(columnOf('ready').querySelector('.board-column-head').style.backgroundColor).toBe('rgb(29, 78, 216)')
    expect(titlesIn('ready')).toEqual(['one', 'three'])
  })

  it('moves an entry whose value is picked from its own chip', async () => {
    render()
    app.reevaluateDocumentSelective = vi.fn(async () => {})
    entryAt('two').querySelector('.enum-field .tag-chip').dispatchEvent(new Event('click', { bubbles: true }))
    Array.from(document.querySelectorAll('.tag-picker .tag-option'))
      .find((o) => o.textContent.trim() === 'testing').dispatchEvent(new Event('click', { bubbles: true }))
    await vi.waitFor(() => expect(titlesIn('testing')).toEqual(['two']))
  })

  it('keeps an entry drawn again in its column', () => {
    render()
    app.renderer.rerenderSubtree(app.currentDocument, ['s', 'Items', 'three'])
    expect(titlesIn('ready')).toEqual(['one', 'three'])
  })

  it('counts only what the filter leaves in a column', () => {
    render()
    entryAt('one').classList.add('filtered-out')
    app.renderer._filters = new Map([['f', { target: ['s', 'Nothing'] }]])
    app.renderer.applyFilters()
    expect(columnOf('ready').querySelector('.board-column-count').textContent).toBe('1')
  })
})
