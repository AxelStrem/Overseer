import { describe, it, expect, beforeEach, vi } from 'vitest'

// A list whose entries came or went is answered with its entries, not with itself.
//
// Marking a task done used to bring back the whole history and the whole open list - 4.5 MB of
// JSON on tasks.os - and the page then drew the whole tab again, since the parent of a list sent
// whole is what knows where it goes. Now the answer names the list's entries in their new order,
// each kept from where it was or sent new, and the page draws that list and nothing around it.
//
// A kept entry comes with its name: entries are named by their place, so the ones after an entry
// taken out are renamed, and a button on one of them has to say the new name when it is pressed.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
      </div>
      <button id="welcome-open-btn"></button>
      <button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div>
      <div id="content-display"></div>
      <div id="status-bar">
        <span id="status-message"></span>
        <span id="status-info"></span>
        <span id="file-path"></span>
      </div>
      <div id="welcome-screen" class="screen"></div>
      <div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div>
      <div id="error-message"></div>
    </div>
  `
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, variant, value) => Object.assign(base(name, type), {
  parameters: { value: { [variant]: value } },
})

const doneButton = () => Object.assign(base('done', 'button'), {
  parameters: { value: { String: 'done' } },
  children: [base('click', 'on')],
})

const task = (position, entry) => Object.assign(base(`Task__${position}`, 'div'), {
  parameters: { _ui_sort_key: { Integer: entry.sort } },
  children: [
    valued('added', 'timestamp', 'Timestamp', entry.key),
    valued('title', 'string', 'String', entry.text),
    doneButton(),
  ],
})

const ENTRIES = [
  { key: '2026-08-01T09:00:00Z', text: 'urgent', sort: -38 },
  { key: '2026-08-02T09:00:00Z', text: 'middling', sort: -13 },
  { key: '2026-08-03T09:00:00Z', text: 'calm', sort: 0 },
]

const documentOf = () => ([Object.assign(base('tasks', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    valued('note', 'string', 'String', 'drawn once'),
    Object.assign(base('Open', 'list'), {
      parameters: { key: { String: 'added' } },
      children: ENTRIES.map((e, i) => task(i + 1, e)),
    }),
  ],
})])

const buttons = () => Array.from(document.querySelectorAll('[data-path]'))
  .filter(e => { try { return JSON.parse(e.dataset.path).slice(-1)[0] === 'done' } catch { return false } })

const rowsOnScreen = () => buttons().map((b) => {
  const path = JSON.parse(b.dataset.path)
  const el = document.querySelector(`[data-path='${JSON.stringify([...path.slice(0, -1), 'title'])}']`)
  return (el?.textContent || '').trim()
})

const noteOnScreen = () => document.querySelector(`[data-path='${JSON.stringify(['tasks', 'note'])}']`)

describe('a list answered with its entries', () => {
  let app, answer, sent

  beforeEach(async () => {
    setupDOM()
    sent = []
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(async (cmd, args) => {
      if (cmd !== 'execute_overseer_event_update') return null
      sent.push((args.node_path || [])[2])
      return { text: `TEXT-${sent.length}`, changes: answer(), nodes: null }
    })
    app = new OverseerApp()
    app.currentDocument = documentOf()
    app._currentText = 'TEXT-BEFORE'
    app.renderer.renderDocument(app.currentDocument)
  })

  it('keeps the entries that stayed, renamed, and draws the list and nothing around it', async () => {
    const note = noteOnScreen()
    const [urgent, , calm] = app.currentDocument[0].children[1].children
    // `middling` done: the third entry is second now, and called so.
    answer = () => [{
      kind: 'entries', address: 'tasks/Open', path: [0, 1],
      entries: [{ kept: 0, name: 'Task__1' }, { kept: 2, name: 'Task__2' }],
    }]
    buttons()[1].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(sent).toEqual(['Task__2'])
    expect(rowsOnScreen()).toEqual(['urgent', 'calm'])
    const open = app.currentDocument[0].children[1].children
    expect(open[0], 'an entry that stayed was replaced rather than kept').toBe(urgent)
    expect(open[1]).toBe(calm)
    expect(open.map(e => e.name)).toEqual(['Task__1', 'Task__2'])
    expect(
      buttons().map(b => JSON.parse(b.dataset.path)[2]),
      'the page is still offering the names the entries had before'
    ).toEqual(['Task__1', 'Task__2'])
    expect(noteOnScreen(), 'what sits beside the list was drawn again').toBe(note)
  })

  it('puts an entry sent new where the list sorts it, and draws only that', async () => {
    const note = noteOnScreen()
    const drawn = buttons()
    const fresh = task(4, { key: '2026-08-04T09:00:00Z', text: 'sooner still', sort: -50 })
    answer = () => [{
      kind: 'entries', address: 'tasks/Open', path: [0, 1],
      entries: [{ kept: 0, name: 'Task__1' }, { kept: 1, name: 'Task__2' }, { kept: 2, name: 'Task__3' }, { node: fresh }],
    }]
    buttons()[2].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(rowsOnScreen()).toEqual(['sooner still', 'urgent', 'middling', 'calm'])
    expect(noteOnScreen()).toBe(note)
    expect(buttons().slice(1), 'the entries already on screen were drawn again').toEqual(drawn)
  })

  it('applies what changed inside a kept entry after the entries, where it now is', async () => {
    // Paths inside the list count in the new order: the entry that was third is at index 1.
    answer = () => [
      {
        kind: 'entries', address: 'tasks/Open', path: [0, 1],
        entries: [{ kept: 0, name: 'Task__1' }, { kept: 2, name: 'Task__2' }],
      },
      {
        kind: 'parameters', address: 'tasks/Open/[2026-08-03T09:00:00Z]/title', path: [0, 1, 1, 1],
        parameters: { value: { String: 'calm, and moved up' } },
      },
    ]
    buttons()[1].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(rowsOnScreen()).toEqual(['urgent', 'calm, and moved up'])
  })
})

// What a change does not show is not drawn again - and never the whole document for it.
//
// Marking a task done moves counts kept on the hidden template of a rule, and marks its lists and
// the tab as overridden for the serializer. The first had no place on screen, so the page drew the
// whole document; the second drew the tab. Either way a press ended in drawing everything.
describe('a change the screen does not show', () => {
  let app, renders

  const field = (name, variant, value, hidden) => {
    const node = valued(name, 'string', variant, value)
    if (hidden) node.parameters.hidden = { Boolean: true }
    return node
  }
  const at = (...path) => document.querySelector(`[data-path='${JSON.stringify(path)}']`)
  const apply = (changes) => {
    const touched = app.applyDocumentChanges(app.currentDocument, changes)
    app.renderer.repaintNodes(touched, app.currentDocument)
  }

  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
    app.currentDocument = [Object.assign(base('tasks', 'tab'), {
      parameters: { mutable: { Boolean: true } },
      children: [
        Object.assign(base('templates', 'div'), {
          parameters: { hidden: { Boolean: true } },
          children: [Object.assign(base('Rule', 'div'), { children: [field('since_done', 'Integer', 3, true)] })],
        }),
        field('note', 'String', 'drawn once'),
        field('later', 'String', 'not yet', true),
      ],
    })]
    app.renderer.renderDocument(app.currentDocument)
    renders = 0
    const render = app.renderer.renderDocument.bind(app.renderer)
    app.renderer.renderDocument = (d) => { renders += 1; return render(d) }
  })

  it('leaves a field of a hidden template alone', () => {
    const note = at('tasks', 'note')
    apply([{ kind: 'parameters', address: 'tasks/templates/Rule/since_done', path: [0, 0, 0, 0],
      parameters: { value: { Integer: 3 }, hidden: { Boolean: true }, _computed_value: { Integer: 0 } } }])
    expect(renders, 'the whole document was drawn').toBe(0)
    expect(at('tasks', 'note')).toBe(note)
    expect(app.currentDocument[0].children[0].children[0].children[0].parameters._computed_value).toEqual({ Integer: 0 })
  })

  it('draws nothing for what only the serializer reads', () => {
    const note = at('tasks', 'note')
    apply([{ kind: 'parameters', address: 'tasks', path: [0],
      parameters: { mutable: { Boolean: true }, _explicit_overrides: { String: 'Open' } } }])
    expect(renders).toBe(0)
    expect(at('tasks', 'note'), 'the tab was drawn again for a note to the serializer').toBe(note)
    expect(app.currentDocument[0].parameters._explicit_overrides).toEqual({ String: 'Open' })
  })

  it('takes a field off when it is hidden and puts one on when it is shown', () => {
    apply([{ kind: 'parameters', address: 'tasks/note', path: [0, 1],
      parameters: { value: { String: 'drawn once' }, hidden: { Boolean: true } } }])
    expect(at('tasks', 'note'), 'a field hidden since is still on screen').toBeNull()

    apply([{ kind: 'parameters', address: 'tasks/later', path: [0, 2],
      parameters: { value: { String: 'now' } } }])
    expect(at('tasks', 'later'), 'a field shown since is not on screen').not.toBeNull()
    expect(renders, 'the whole document was drawn').toBe(0)
  })
})

// An entry whose place in the order moves, with nothing added or taken out. Its sort key comes
// as a change to its parameters, and the repaint drew it again where it stood - so an exercise
// marked done stayed at the top of a list sorted by when each was last done, until a reload. It
// looked right before only because a new history entry used to redraw the whole page.
describe('an entry that moves in the order', () => {
  let app, answer

  beforeEach(async () => {
    setupDOM()
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(async (cmd) => cmd === 'execute_overseer_event_update'
      ? { text: 'TEXT-AFTER', changes: answer(), nodes: null }
      : null)
    app = new OverseerApp()
    app.currentDocument = documentOf()
    app._currentText = 'TEXT-BEFORE'
    app.renderer.renderDocument(app.currentDocument)
  })

  const keyed = (position, sort) => ({
    kind: 'parameters', address: `tasks/Open/[${ENTRIES[position].key}]`, path: [0, 1, position],
    parameters: { _ui_sort_key: { Integer: sort } },
  })

  it('goes to its new place when its sort key is all that changed', async () => {
    const note = noteOnScreen()
    const [, middling, calm] = buttons()
    // `urgent` done, and sorted after everything now.
    answer = () => [keyed(0, 10)]
    buttons()[0].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(rowsOnScreen()).toEqual(['middling', 'calm', 'urgent'])
    expect(buttons().slice(0, 2), 'the entries that did not move were drawn again').toEqual([middling, calm])
    expect(noteOnScreen(), 'what sits beside the list was drawn again').toBe(note)
  })

  it('goes to its new place when it is drawn again as well', async () => {
    // The usual case: what moves it - the time it was last done - is on it, and shown.
    answer = () => [
      keyed(0, 10),
      {
        kind: 'parameters', address: `tasks/Open/[${ENTRIES[0].key}]/title`, path: [0, 1, 0, 1],
        parameters: { value: { String: 'urgent, done' } },
      },
    ]
    buttons()[0].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(rowsOnScreen()).toEqual(['middling', 'calm', 'urgent, done'])
  })

  it('leaves a list whose order did not change as it was', async () => {
    const drawn = buttons()
    answer = () => [keyed(1, -20)]
    buttons()[1].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(rowsOnScreen()).toEqual(['urgent', 'middling', 'calm'])
    expect(buttons()).toEqual(drawn)
  })
})
