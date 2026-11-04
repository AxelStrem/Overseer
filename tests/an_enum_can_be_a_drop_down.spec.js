import { describe, it, expect, beforeEach, vi } from 'vitest'

// `view="select"` draws an enum as a drop-down's box rather than a chip. Only the drawing
// differs: the picker, what it leaves out, and the write are the chip's.

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
import { invoke } from '@tauri-apps/api/core'
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
  parameters: { value: { String: held }, vocabulary: { String: '/project/Stages' },
                view: { String: 'select' } },
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

const boxes = () => Array.from(document.querySelectorAll('.enum-field .enum-select'))
const offered = () => Array.from(document.querySelectorAll('.tag-picker .tag-option'))
  .map(o => o.textContent.trim())
const press = (el) => el.dispatchEvent(new Event('click', { bubbles: true }))

describe('an enum drawn as a drop-down', () => {
  let app

  const render = (doc) => {
    app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('is a box holding the chosen name, not a chip', () => {
    render(docWith('ready'))
    expect(boxes().map(b => b.textContent.trim())).toEqual(['ready to go'])
    expect(document.querySelectorAll('.enum-field .tag-chip')).toHaveLength(0)
    expect(boxes()[0].classList.contains('enum-choice')).toBe(true)
    expect(boxes()[0].style.backgroundColor).toBe('')
  })

  it('says its full name even when its chip would be only an icon', () => {
    const doc = docWith('ready')
    doc[0].children[1].parameters.chip = { String: 'icon' }
    render(doc)
    expect(boxes()[0].textContent.trim()).toBe('ready to go')
  })

  it('shows its placeholder when nothing is chosen', () => {
    const doc = docWith('')
    doc[0].children[1].parameters.placeholder = { String: 'Pick one' }
    render(doc)
    expect(boxes()[0].textContent.trim()).toBe('Pick one')
    expect(boxes()[0].classList.contains('enum-select-empty')).toBe(true)
  })

  it('marks a value the list does not hold', () => {
    render(docWith('shipped'))
    expect(boxes()[0].textContent.trim()).toBe('shipped')
    expect(boxes()[0].classList.contains('tag-unknown')).toBe(true)
  })

  it('opens the picker with the other values, holding back what it is told to', () => {
    const doc = docWith('ready')
    doc[0].children[1].parameters.withhold = { String: 'testing' }
    render(doc)
    press(boxes()[0])
    expect(offered()).toEqual(['filed'])
  })

  it('writes the value picked, as the chip does', async () => {
    render(docWith('filed'))
    const writes = []
    app.reevaluateDocumentSelective = vi.fn(async (paths, changes) => { writes.push(changes) })
    press(boxes()[0])
    press(Array.from(document.querySelectorAll('.tag-picker .tag-option'))
      .find(o => o.textContent.trim() === 'testing'))
    await vi.waitFor(() => expect(writes).toHaveLength(1))

    expect(writes[0]).toEqual([{ path: 'project/stage', oldValue: 'filed', newValue: 'testing' }])
    expect(boxes().map(b => b.textContent.trim())).toEqual(['testing'])
  })

  it('sends the value with its change handler, as one instruction, when there is a file', async () => {
    const doc = docWith('filed')
    doc[0].children[1].children = [Object.assign(base('change', 'on'), { children: [base('set', 'set')] })]
    render(doc)
    app.currentFile = 'p.os'
    const sent = []
    invoke.mockImplementation(async (cmd, args) => { sent.push([cmd, args]); return null })
    app.reevaluateDocumentSelective = vi.fn(async () => {})
    press(boxes()[0])
    press(Array.from(document.querySelectorAll('.tag-picker .tag-option'))
      .find(o => o.textContent.trim() === 'testing'))
    await vi.waitFor(() => expect(sent.some(([cmd]) => cmd === 'run_overseer_event')).toBe(true))

    const [, args] = sent.find(([cmd]) => cmd === 'run_overseer_event')
    expect(args.event_name).toBe('change')
    expect(args.writing).toEqual([{ node_path: ['project', 'stage'], value: { String: 'testing' } }])
    invoke.mockReset()
  })

  it('offers nothing and has no caret when the document is not mutable', () => {
    render(docWith('ready', { mutable: false }))
    press(boxes()[0])
    expect(document.querySelector('.tag-picker')).toBeNull()
    expect(boxes()[0].classList.contains('enum-choice')).toBe(false)
  })
})
