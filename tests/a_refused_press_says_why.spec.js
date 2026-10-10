import { describe, it, expect, beforeEach, vi } from 'vitest'

// A press the document refuses - `refuse = "..."` - says why, beside what was pressed.
//
// Refused used to look exactly like a press that never registered: nothing changed, nothing was
// said. Now the backend answers with the document's sentence, the page shows it next to the
// button, keeps what was typed so it can be put right, and does not ask again the long way. The
// sentence goes as soon as something is typed or pressed.

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

const theDocument = () => [base('p', 'tab', { mutable: { Boolean: true } }, [
  base('NewTask', 'div', { layout: { String: 'horizontal' } }, [
    base('handle', 'textbox', { value: { String: '' }, placeholder: { String: 'id' } }),
    base('add', 'button', { label: { String: 'add' } }, [
      base('click', 'on', {}, [base('refuse', 'refuse', { value: { String: 'say an id' } })]),
    ]),
  ]),
])]

let app

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app.currentFile = 'p.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const box = () => document.querySelector(`[data-path='${JSON.stringify(['p', 'NewTask', 'handle'])}'] input.textbox-input`)
const button = () => document.querySelector(`[data-path='${JSON.stringify(['p', 'NewTask', 'add'])}']`)
const said = () => [...document.querySelectorAll('.overseer-refusal')].map((n) => n.textContent)

const type = (input, text) => {
  input.value = text
  input.dispatchEvent(new window.Event('input', { bubbles: true }))
}

const pressAdd = () => app.renderer.emitEvent(app.currentDocument[0].children[0].children[1], button(), 'click')

// The desktop app hands the error over as serde writes it; the browser's bridge as an Error with
// the server's `refused` on it.
const refusals = {
  'the desktop app': () => ({ Refused: 'this id is taken, say another' }),
  'the browser': () => Object.assign(new Error('this id is taken, say another'), { refused: 'this id is taken, say another' }),
}

for (const [door, refusal] of Object.entries(refusals)) {
  describe(`a refused press, through ${door}`, () => {
    beforeEach(() => {
      setupDOM()
      invoke.mockReset()
      invoke.mockImplementation(async (cmd) => {
        if (cmd === 'run_overseer_event') throw refusal()
        return null
      })
    })

    it('shows the sentence beside what was pressed', async () => {
      render()
      type(box(), 'taken')
      await pressAdd()
      expect(said()).toEqual(['this id is taken, say another'])
      expect(button().nextElementSibling.classList.contains('overseer-refusal')).toBe(true)
    })

    it('keeps what was typed and asks nothing more', async () => {
      render()
      type(box(), 'taken')
      await pressAdd()
      expect(box().value).toBe('taken')
      expect(invoke.mock.calls.map(([cmd]) => cmd), 'the press was asked again the long way').toEqual(['run_overseer_event'])
    })

    it('says it once, however often it is pressed', async () => {
      render()
      await pressAdd()
      await pressAdd()
      expect(said()).toEqual(['this id is taken, say another'])
    })

    it('stops saying it once something is typed', async () => {
      render()
      await pressAdd()
      type(box(), 'fresh')
      expect(said()).toEqual([])
    })
  })
}

describe('a press that is let through', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
  })

  it('takes away what a refused one said', async () => {
    render()
    invoke.mockImplementation(async (cmd) => {
      if (cmd === 'run_overseer_event') throw { Refused: 'say an id' }
      return null
    })
    await pressAdd()
    expect(said()).toEqual(['say an id'])

    invoke.mockImplementation(async (cmd) =>
      cmd === 'run_overseer_event' ? { text: 'TEXT-2', changes: [], nodes: null, wrote: true, emptied: [] } : null)
    await pressAdd()
    expect(said()).toEqual([])
  })

  it('says nothing when it fails for another reason', async () => {
    render()
    invoke.mockImplementation(async (cmd) => {
      if (cmd === 'run_overseer_event') throw { Failed: 'could not run the event' }
      return null
    })
    await pressAdd().catch(() => {})
    expect(said()).toEqual([])
  })
})
