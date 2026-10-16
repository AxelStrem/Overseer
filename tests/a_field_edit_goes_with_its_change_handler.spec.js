import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

// A field can say what happens when it changes, with `on change`, and an edit typed into it runs
// that handler. With a file, the value travels with the handler and the two are written as one
// change, as an enum's pick and a markdown edit are.
//
// Typed into a plain field, the handler never ran: the edit went up as a selective write, and a
// write that succeeded - nearly every one - also skipped the field's change event, which had been
// put there to keep charts from redrawing. When the handler did run, it went after the value as a
// second write: two steps to take back, and the race in which a comment's text was lost.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        <button id="undo-file-btn"></button></div>
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

const handler = () => ({
  name: 'change', node_type: 'on', parameters: {}, is_hierarchy_transparent: false,
  children: [{ name: 'set', node_type: 'set', parameters: {}, children: [], is_hierarchy_transparent: false }],
})

const field = (name, type, value, handled) => ({
  name, node_type: type,
  parameters: { mutable: { Boolean: true }, value },
  is_hierarchy_transparent: false,
  children: handled ? [handler()] : [],
})

const aDay = () => [{
  name: 'day', node_type: 'tab', parameters: { mutable: { Boolean: true } }, is_hierarchy_transparent: false,
  children: [
    field('name', 'string', { String: 'as it was' }, true),
    field('count', 'int', { Integer: 1 }, true),
    field('free', 'string', { String: 'as it was' }, false),
  ],
}]

const anAppShowingADay = (file = '/documents/day.os') => {
  const app = new OverseerApp()
  window.app = app
  app.currentFile = file
  app._currentText = 'TEXT'
  app._originalText = 'TEXT'
  app._adoptDocumentPreserveRoot(aDay())
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

/** Open the field for editing, type, and press Enter - as a person does. */
const typeInto = (name, text) => {
  const el = Array.from(document.querySelectorAll('[data-path]')).find((e) => {
    try { return JSON.parse(e.dataset.path || '[]').slice(-1)[0] === name } catch { return false }
  })
  const holder = el.querySelector('.field-value, .text-content') || el
  holder.dispatchEvent(new Event('dblclick', { bubbles: true }))
  const input = document.querySelector('input.field-editor, textarea.field-editor')
  expect(input, 'the editor did not open').toBeTruthy()
  input.value = text
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
}

const callsTo = (name) => invoke.mock.calls.filter(([cmd]) => cmd === name)

const settle = async () => {
  for (let i = 0; i < 20; i += 1) await new Promise((r) => setTimeout(r, 0))
}

describe('editing a field with a change handler', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation((cmd) =>
      cmd === 'write_overseer_values'
        ? Promise.resolve({ text: 'TEXT', changes: [], nodes: null })
        : Promise.resolve(null))
  })
  afterEach(() => { delete window.app })

  it('sends the value with the handler, as one instruction', async () => {
    anAppShowingADay()
    typeInto('name', 'as it is now')
    await settle()

    const ran = callsTo('run_overseer_event')
    expect(ran, 'the handler was not asked for').toHaveLength(1)
    expect(ran[0][1].event_name).toBe('change')
    expect(ran[0][1].node_path).toEqual(['day', 'name'])
    expect(ran[0][1].writing).toEqual([{ node_path: ['day', 'name'], value: { String: 'as it is now' } }])
    expect(callsTo('write_overseer_values'), 'the value went separately as well').toHaveLength(0)
  })

  it('sends a number as the number it is', async () => {
    anAppShowingADay()
    typeInto('count', '7')
    await settle()

    const ran = callsTo('run_overseer_event')
    expect(ran).toHaveLength(1)
    expect(ran[0][1].writing).toEqual([{ node_path: ['day', 'count'], value: { Integer: 7 } }])
  })

  it('leaves a field without a handler to the write it always made', async () => {
    anAppShowingADay()
    typeInto('free', 'as it is now')
    await settle()

    expect(callsTo('run_overseer_event'), 'a handler nobody declared was asked for').toHaveLength(0)
    const sent = callsTo('write_overseer_values')
    expect(sent).toHaveLength(1)
    expect(sent[0][1].values).toEqual([{ node_path: ['day', 'free'], value: { String: 'as it is now' } }])
  })

  it('still runs the handler, after the value, with no file to write to', async () => {
    const app = anAppShowingADay(null)
    const order = []
    const reevaluate = app.reevaluateDocumentSelective.bind(app)
    app.reevaluateDocumentSelective = vi.fn(async (...args) => {
      order.push('value')
      await reevaluate(...args)
      return { success: true }
    })
    const emit = app.renderer.emitEvent.bind(app.renderer)
    app.renderer.emitEvent = vi.fn(async (node, holder, name, value) => {
      order.push(name + (value === undefined ? '' : ' with value'))
      return emit(node, holder, name, value)
    })
    typeInto('name', 'as it is now')
    await settle()

    expect(order.slice(0, 2)).toEqual(['value', 'change'])
  })
})
