import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A page takes a write made elsewhere, without a reload.
//
// The server tells a page each time its document's file changes - the bot recording something
// in a document somebody has open, most of the day - and the page asks to be caught up: an answer
// described against what it holds, taken the way an edit's answer is. Never over what somebody is
// part-way through: an edit on its way, a field open for editing, a box typed into.

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
import { versionOf } from '../src/text_version.js'

const TEXT = 'tab day (mutable=true) {\n    int n = 1\n    textbox note = ""\n}\n'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const theDocument = () => [base('day', 'tab', { mutable: { Boolean: true } }, [
  base('n', 'int', { value: { Integer: 1 }, label: { String: 'N' } }),
  base('note', 'textbox', { value: { String: '' } }),
])]

let app

const anApp = () => {
  app = new OverseerApp()
  app.currentFile = 'day.os'
  app._currentText = TEXT
  app._originalText = TEXT
  app.currentDocument = theDocument()
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

// What the server answers a catch-up after the bot set `n` to 9: the field's new parameters.
const theBotsWrite = (version = 'bot-1') => ({
  version,
  changes: [{ kind: 'parameters', path: [0, 0], parameters: { value: { Integer: 9 }, label: { String: 'N' } } }],
  nodes: null,
  wrote: false,
})

const catchUps = () => invoke.mock.calls.filter(([cmd]) => cmd === 'catch_up_overseer_document')

const answering = (answer) => invoke.mockImplementation(async (cmd) =>
  cmd === 'catch_up_overseer_document' ? answer : null)

const shownN = () => document.querySelector(`[data-path='${JSON.stringify(['day', 'n'])}'] .field-value`)?.textContent

const settle = async (ms = 0) => { await vi.advanceTimersByTimeAsync(ms) }

describe('a page takes a write made elsewhere', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
  })
  afterEach(() => { vi.useRealTimers() })

  it('shows what the other writer wrote, without a reload', async () => {
    anApp()
    answering(theBotsWrite())
    expect(shownN()).toBe('1')
    app.heardTheFile('bot-1')
    await settle()
    expect(catchUps()).toHaveLength(1)
    // Described against the text the page holds, named once.
    expect(catchUps()[0][1].shown_version).toBe(versionOf(TEXT))
    expect(shownN(), 'the bot\'s value never reached the page').toBe('9')
  })

  it('takes the file as the baseline a later save is checked against', async () => {
    anApp()
    answering(theBotsWrite('bot-1'))
    app.heardTheFile('bot-1')
    await settle()
    expect(app.fileVersion()).toBe('bot-1')
    expect(app.showing().shown_version).toBe('bot-1')
    // Heard again on reconnecting: nothing to do.
    app.heardTheFile('bot-1')
    await settle()
    expect(catchUps()).toHaveLength(1)
  })

  it('ignores its own write told back to it', async () => {
    anApp()
    app.alreadyWritten({ wrote: true, version: 'mine-1' })
    app.heardTheFile('mine-1')
    await settle(1000)
    expect(catchUps()).toHaveLength(0)
    // And the version it already holds, which the server says first as the page starts listening.
    app.heardTheFile('mine-1')
    await settle(1000)
    expect(catchUps()).toHaveLength(0)
  })

  it('waits while a field is open for editing, then catches up', async () => {
    anApp()
    answering(theBotsWrite())
    const editor = document.createElement('input')
    editor.className = 'field-editor'
    document.getElementById('content-display').appendChild(editor)
    app.heardTheFile('bot-1')
    await settle(2000)
    expect(catchUps(), 'caught up under an open editor').toHaveLength(0)
    editor.remove()
    await settle(400)
    expect(catchUps()).toHaveLength(1)
    expect(shownN()).toBe('9')
  })

  it('waits for an edit on its way, whose own answer brings the other write', async () => {
    anApp()
    answering(theBotsWrite())
    app._writingByInstruction = 1
    app.heardTheFile('bot-1')
    await settle(1000)
    expect(catchUps()).toHaveLength(0)
    // The edit's answer was described against what the page holds, and said what the file is now.
    app.alreadyWritten({ wrote: true, version: 'bot-1-and-mine' })
    app.heardTheFile('bot-1-and-mine')
    app._writingByInstruction = 0
    await settle(1000)
    expect(catchUps(), 'caught up with what the edit\'s answer already brought').toHaveLength(0)
  })

  it('drops an answer that arrives after an edit began, and asks again', async () => {
    anApp()
    let release
    invoke.mockImplementation((cmd) => cmd === 'catch_up_overseer_document'
      ? new Promise((resolve) => { release = resolve })
      : Promise.resolve(null))
    app.heardTheFile('bot-1')
    await settle()
    expect(catchUps()).toHaveLength(1)
    app._writingByInstruction = 1
    release(theBotsWrite())
    await settle()
    expect(shownN(), 'an answer worked out before the edit was drawn over it').toBe('1')
    app._writingByInstruction = 0
    answering(theBotsWrite())
    await settle(400)
    expect(catchUps()).toHaveLength(2)
    expect(shownN()).toBe('9')
  })

  it('keeps what is typed into a textbox', async () => {
    anApp()
    const box = () => document.querySelector(`[data-path='${JSON.stringify(['day', 'note'])}'] input.textbox-input`)
    box().value = 'half a sentence'
    box().dispatchEvent(new window.Event('input', { bubbles: true }))
    // A change that draws the whole tab again.
    answering({
      version: 'bot-1',
      changes: [{ kind: 'parameters', path: [0], parameters: { mutable: { Boolean: true }, label: { String: 'Day' } } }],
      nodes: null,
    })
    app.heardTheFile('bot-1')
    await settle()
    expect(catchUps()).toHaveLength(1)
    expect(box().value).toBe('half a sentence')
  })

  it('rebuilds from a whole document when the server has nothing to describe against', async () => {
    anApp()
    const NOW = TEXT.replace('int n = 1', 'int n = 9')
    answering({
      text: NOW, version: versionOf(NOW), changes: null,
      nodes: [base('day', 'tab', { mutable: { Boolean: true } }, [
        base('n', 'int', { value: { Integer: 9 }, label: { String: 'N' } }),
        base('note', 'textbox', { value: { String: '' } }),
      ])],
    })
    app.heardTheFile(versionOf(NOW))
    await settle()
    expect(shownN()).toBe('9')
    expect(app.fileVersion()).toBe(versionOf(NOW))
  })
})
