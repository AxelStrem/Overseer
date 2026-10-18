import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

// A box ticked by hand runs the field's `on change`, as a typed value and a picked chip do, and
// with a file the value travels with the handler, so the two are one change and one undo.
//
// A `bool` never ran its handler at all: its listener wrote the value and stopped. A `checkbox`
// wrote the value and then asked for its events without it, as separate writes that raced.

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

const handler = (event) => ({
  name: event, node_type: 'on', parameters: {}, is_hierarchy_transparent: false,
  children: [{ name: 'set', node_type: 'set', parameters: {}, children: [], is_hierarchy_transparent: false }],
})

const box = (name, type, events) => ({
  name, node_type: type,
  parameters: { mutable: { Boolean: true }, value: { Boolean: false } },
  is_hierarchy_transparent: false,
  children: events.map(handler),
})

const aDay = () => [{
  name: 'day', node_type: 'tab', parameters: { mutable: { Boolean: true } }, is_hierarchy_transparent: false,
  children: [
    box('done', 'bool', ['change']),
    box('ticked', 'checkbox', ['change']),
    box('both', 'checkbox', ['check', 'change']),
    box('free', 'bool', []),
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

/** Tick the box of the field with this name, as a person does. */
const tick = (name) => {
  const el = Array.from(document.querySelectorAll('[data-path]')).find((e) => {
    try { return JSON.parse(e.dataset.path || '[]').slice(-1)[0] === name } catch { return false }
  })
  const input = el.querySelector('input[type="checkbox"]')
  expect(input, 'no box drawn').toBeTruthy()
  input.checked = !input.checked
  input.dispatchEvent(new Event('change', { bubbles: true }))
}

const callsTo = (name) => invoke.mock.calls.filter(([cmd]) => cmd === name)

const settle = async () => {
  for (let i = 0; i < 20; i += 1) await new Promise((r) => setTimeout(r, 0))
}

describe('ticking a box with a change handler', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation((cmd) =>
      cmd === 'write_overseer_values'
        ? Promise.resolve({ text: 'TEXT', changes: [], nodes: null })
        : Promise.resolve(null))
  })
  afterEach(() => { delete window.app })

  for (const name of ['done', 'ticked']) {
    it(`sends the ${name === 'done' ? 'bool' : 'checkbox'}'s value with the handler, as one instruction`, async () => {
      anAppShowingADay()
      tick(name)
      await settle()

      const ran = callsTo('run_overseer_event')
      expect(ran, 'the handler was not asked for').toHaveLength(1)
      expect(ran[0][1].event_name).toBe('change')
      expect(ran[0][1].node_path).toEqual(['day', name])
      expect(ran[0][1].writing).toEqual([{ node_path: ['day', name], value: { Boolean: true } }])
      expect(callsTo('write_overseer_values'), 'the value went separately as well').toHaveLength(0)
    })
  }

  it('sends the value once, with the first of a checkbox\'s handlers', async () => {
    anAppShowingADay()
    tick('both')
    await settle()

    const ran = callsTo('run_overseer_event')
    expect(ran.map(([, a]) => a.event_name)).toEqual(['check', 'change'])
    expect(ran[0][1].writing).toEqual([{ node_path: ['day', 'both'], value: { Boolean: true } }])
    expect(ran[1][1].writing).toBeUndefined()
    expect(callsTo('write_overseer_values')).toHaveLength(0)
  })

  it('leaves a box without a handler to the write it always made', async () => {
    anAppShowingADay()
    tick('free')
    await settle()

    expect(callsTo('run_overseer_event'), 'a handler nobody declared was asked for').toHaveLength(0)
    const sent = callsTo('write_overseer_values')
    expect(sent).toHaveLength(1)
    expect(sent[0][1].values).toEqual([{ node_path: ['day', 'free'], value: { Boolean: true } }])
  })
})
