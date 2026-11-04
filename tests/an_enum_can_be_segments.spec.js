import { describe, it, expect, beforeEach, vi } from 'vitest'

// `view="segments"` draws an enum as one row of buttons, a value each, the held one pressed.
// Only the drawing differs from the chip: what is offered and how a choice is written are its.

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
                view: { String: 'segments' } },
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

const segments = () => Array.from(document.querySelectorAll('.enum-field .enum-segment'))
const names = () => segments().map(s => s.textContent.trim())
const chosen = () => segments().filter(s => s.classList.contains('chosen')).map(s => s.textContent.trim())
const press = (el) => el.dispatchEvent(new Event('click', { bubbles: true }))
const segment = (name) => segments().find(s => s.textContent.trim() === name)

describe('an enum drawn as segments', () => {
  let app

  const render = (doc) => {
    app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('draws a button for each value in the order the list gives them, not a chip', () => {
    render(docWith('ready'))
    expect(names()).toEqual(['filed', 'ready to go', 'testing'])
    expect(document.querySelectorAll('.enum-field .tag-chip')).toHaveLength(0)
  })

  it('presses the one it holds, filled with its colour', () => {
    render(docWith('ready'))
    expect(chosen()).toEqual(['ready to go'])
    expect(segment('ready to go').getAttribute('aria-pressed')).toBe('true')
    expect(segment('ready to go').style.backgroundColor).toBe('rgb(29, 78, 216)')
  })

  it('presses none when it holds a value the list does not', () => {
    render(docWith('shipped'))
    expect(names()).toEqual(['filed', 'ready to go', 'testing'])
    expect(chosen()).toEqual([])
  })

  it('greys out what it is told to withhold, and a press there does nothing', () => {
    const doc = docWith('filed')
    doc[0].children[1].parameters.withhold = { String: 'testing' }
    render(doc)
    app.reevaluateDocumentSelective = vi.fn(async () => {})
    expect(segment('testing').disabled).toBe(true)
    expect(segment('testing').classList.contains('enum-segment-held')).toBe(true)
    press(segment('testing'))
    expect(app.currentDocument[0].children[1].parameters.value).toEqual({ String: 'filed' })
  })

  it('writes the value pressed, as a pick does', async () => {
    render(docWith('filed'))
    const writes = []
    app.reevaluateDocumentSelective = vi.fn(async (paths, changes) => { writes.push(changes) })
    press(segment('testing'))
    await vi.waitFor(() => expect(writes).toHaveLength(1))

    expect(writes[0]).toEqual([{ path: 'project/stage', oldValue: 'filed', newValue: 'testing' }])
    expect(chosen()).toEqual(['testing'])
  })

  it('writes nothing when the pressed one is already held', () => {
    render(docWith('filed'))
    app.reevaluateDocumentSelective = vi.fn(async () => {})
    press(segment('filed'))
    expect(app.reevaluateDocumentSelective).not.toHaveBeenCalled()
  })

  it('sends the value with its change handler, as one instruction, when there is a file', async () => {
    const doc = docWith('filed')
    doc[0].children[1].children = [Object.assign(base('change', 'on'), { children: [base('set', 'set')] })]
    render(doc)
    app.currentFile = 'p.os'
    const sent = []
    invoke.mockImplementation(async (cmd, args) => { sent.push([cmd, args]); return null })
    app.reevaluateDocumentSelective = vi.fn(async () => {})
    press(segment('testing'))
    await vi.waitFor(() => expect(sent.some(([cmd]) => cmd === 'run_overseer_event')).toBe(true))

    const [, args] = sent.find(([cmd]) => cmd === 'run_overseer_event')
    expect(args.event_name).toBe('change')
    expect(args.writing).toEqual([{ node_path: ['project', 'stage'], value: { String: 'testing' } }])
    invoke.mockReset()
  })

  it('draws them all but lets none be pressed when the document is not mutable', () => {
    render(docWith('ready', { mutable: false }))
    expect(names()).toEqual(['filed', 'ready to go', 'testing'])
    expect(segments().every(s => s.disabled)).toBe(true)
    expect(chosen()).toEqual(['ready to go'])
  })
})
