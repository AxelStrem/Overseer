import { describe, it, expect, beforeEach, vi } from 'vitest'

// An `enum` field holds one value out of a list - a task's stage. It is drawn as the chip a tag
// is, from the same vocabulary list, and pressing it offers the other values in the list's order.
// What these pin beyond the drawing: the value is written before the field's `on change` is
// asked for, and that is asked of the entry the value was picked on, wherever the write put it.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, variant, value) => Object.assign(base(name, type), {
  parameters: { value: { [variant]: value } },
})

/** One value the field may hold: what it is called and what colour it takes. */
const stage = (tag, name, colour) => Object.assign(base(`Stage__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', 'String', tag),
    valued('name', 'string', 'String', name),
    valued('colour', 'string', 'String', colour),
  ],
})

const enumField = (held) => Object.assign(base('stage', 'enum'), {
  parameters: { value: { String: held }, vocabulary: { String: '/project/Stages' } },
})

const docWith = (held, { mutable = true } = {}) => ([
  Object.assign(base('project', 'tab'), {
    parameters: { mutable: { Boolean: mutable } },
    children: [
      Object.assign(base('Stages', 'list'), {
        parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
        children: [stage('filed', 'filed', '#6b7280'),
                   stage('ready', 'ready to go', '#1d4ed8'),
                   stage('testing', 'testing', '#a16207')],
      }),
      enumField(held),
    ],
  }),
])

const chips = () => Array.from(document.querySelectorAll('.enum-field .tag-chip:not(.tag-option)'))
const offered = () => Array.from(document.querySelectorAll('.tag-picker .tag-option'))
  .map(o => o.textContent.trim())
const press = (el) => el.dispatchEvent(new Event('click', { bubbles: true }))

describe('an enum field', () => {
  let app

  const render = (doc) => {
    app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('draws the one value it holds as a chip, by the name and colour the list gives it', () => {
    render(docWith('ready'))
    expect(chips().map(c => c.textContent.trim())).toEqual(['ready to go'])
    expect(chips()[0].style.backgroundColor).toBe('rgb(29, 78, 216)')
  })

  it('shows a value the list does not hold rather than hiding it', () => {
    render(docWith('shipped'))
    expect(chips().map(c => c.textContent.trim())).toEqual(['shipped'])
    expect(chips()[0].classList.contains('tag-unknown')).toBe(true)
  })

  it('shows a place to press when nothing is chosen', () => {
    render(docWith(''))
    expect(chips()).toHaveLength(1)
    expect(chips()[0].classList.contains('tag-placeholder')).toBe(true)
  })

  it('offers the other values in the order the list gives them', () => {
    render(docWith('ready'))
    press(chips()[0])
    expect(offered()).toEqual(['filed', 'testing'])
  })

  it('writes the value picked, saying what it changed from', async () => {
    render(docWith('filed'))
    const writes = []
    app.reevaluateDocumentSelective = vi.fn(async (paths, changes) => { writes.push(changes) })
    press(chips()[0])
    Array.from(document.querySelectorAll('.tag-picker .tag-option'))
      .find(o => o.textContent.trim() === 'testing').dispatchEvent(new Event('click', { bubbles: true }))
    await vi.waitFor(() => expect(writes).toHaveLength(1))

    expect(app.currentDocument[0].children[1].parameters.value).toEqual({ String: 'testing' })
    expect(writes[0]).toEqual([{ path: 'project/stage', oldValue: 'filed', newValue: 'testing' }])
    expect(chips().map(c => c.textContent.trim())).toEqual(['testing'])
  })

  it('asks for its change handler only once the value is written', async () => {
    render(docWith('filed'))
    let finishWrite
    const order = []
    app.reevaluateDocumentSelective = vi.fn(() => new Promise((resolve) => {
      order.push('write sent')
      finishWrite = () => { order.push('write done'); resolve() }
    }))
    app.renderer.emitEvent = vi.fn(async (_node, holder, name) => {
      order.push(`${name} at ${holder.dataset.path}`)
    })
    press(chips()[0])
    press(Array.from(document.querySelectorAll('.tag-picker .tag-option'))[0])
    await vi.waitFor(() => expect(order).toEqual(['write sent']))
    finishWrite()
    await vi.waitFor(() => expect(order).toHaveLength(3))

    expect(order).toEqual(['write sent', 'write done', 'change at ["project","stage"]'])
  })

  it('asks it of the entry it was picked on, after the write has moved it', async () => {
    // A list kept in order of stage puts the entry somewhere else as soon as the stage is
    // written, and the name it was drawn under then belongs to another entry.
    const item = (name, handle, held) => Object.assign(base(name, 'div'), {
      children: [valued('handle', 'string', 'String', handle), enumField(held)],
    })
    const doc = docWith('filed')
    doc[0].children[1] = Object.assign(base('Items', 'list'), {
      parameters: { key: { String: 'handle' } },
      children: [item('Item__0', 'alpha', 'filed'), item('Item__1', 'beta', 'filed')],
    })
    render(doc)
    const events = []
    app.reevaluateDocumentSelective = vi.fn(async () => {
      const list = app.currentDocument[0].children[1]
      list.children = [list.children[1], list.children[0]]
      list.children[0].name = 'Item__0'
      list.children[1].name = 'Item__1'
    })
    app.renderer.emitEvent = vi.fn(async (_node, holder, name) => {
      events.push([name, JSON.parse(holder.dataset.path)])
    })
    press(chips()[0])
    press(Array.from(document.querySelectorAll('.tag-picker .tag-option'))
      .find(o => o.textContent.trim() === 'testing'))
    await vi.waitFor(() => expect(events).toHaveLength(1))

    expect(events[0]).toEqual(['change', ['project', 'Items', 'Item__1', 'stage']])
  })

  it('offers nothing to choose when the document is not mutable', () => {
    render(docWith('ready', { mutable: false }))
    press(chips()[0])
    expect(offered()).toEqual([])
    expect(chips()[0].classList.contains('enum-choice')).toBe(false)
  })
})
