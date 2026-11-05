import { describe, it, expect, beforeEach, vi } from 'vitest'

// `textbox (kind="secret")`: a password. It is drawn masked, no press sends what is typed into
// it, and a `call` that names it gets it from the page, which empties the box afterwards.

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

const base = (name, node_type, parameters = {}, children = [], transparent = false) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: transparent,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

// t / [ password, note, go ]
const PASSWORD = [0, 0]

const theDocument = () => [base('t', 'tab', { mutable: { String: 'guarded' } }, [
  base('password', 'textbox', { kind: { String: 'secret' }, value: { String: '' } }),
  base('note', 'textbox', { value: { String: '' } }),
  base('go', 'button', { label: { String: 'Go' } }, [
    base('click', 'on', {}, [base('call', 'call', { name: { String: 'copy' } })]),
  ]),
])]

let app

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

const answering = (calls) => invoke.mockImplementation(async (cmd) =>
  cmd === 'run_overseer_event' ? { text: 'TEXT', changes: [], nodes: null, wrote: false, calls } : null)

const boxOf = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}'] input`)

const type = (name, text) => {
  const input = boxOf(name)
  input.value = text
  input.dispatchEvent(new Event('input'))
}

const press = async () => {
  const button = app.currentDocument[0].children[2]
  await app.renderer.emitEvent(
    button,
    document.querySelector(`[data-path='${JSON.stringify(['t', 'go'])}']`),
    'click',
  )
}

const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('a secret textbox', () => {
  let clipboard

  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    clipboard = vi.fn(async () => {})
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: clipboard }, configurable: true })
  })

  it('is drawn masked', () => {
    render()
    expect(boxOf('password').type).toBe('password')
    expect(boxOf('note').type).toBe('text')
  })

  it('is never sent with a press', async () => {
    answering([])
    render()
    type('password', 'hunter2')
    type('note', 'kept')
    await press()
    const sent = JSON.stringify(invoke.mock.calls)
    expect(sent).not.toContain('hunter2')
    expect(sent).toContain('kept')
  })

  it('goes to the operation a call names it for, and is emptied after', async () => {
    answering([{ name: 'copy', args: { text: { secret: { address: 't/password', path: PASSWORD } } } }])
    render()
    type('password', 'hunter2')
    await press()
    await settled()
    expect(clipboard).toHaveBeenCalledWith('hunter2')
    expect(app.renderer.typedText()).toEqual([])
    expect(boxOf('password').value).toBe('')
  })

  it('is emptied even when the operation fails', async () => {
    answering([{ name: 'copy', args: { text: { secret: { address: 't/password', path: PASSWORD } } } }])
    clipboard.mockImplementation(async () => { throw new Error('No.') })
    render()
    type('password', 'hunter2')
    await press()
    await settled()
    expect(boxOf('password').value).toBe('')
  })

  it('that the press took away passes nothing', async () => {
    answering([{ name: 'copy', args: { text: { secret: { address: 't/gone', path: null } } } }])
    render()
    type('password', 'hunter2')
    await press()
    await settled()
    expect(clipboard).toHaveBeenCalledWith('')
  })
})
