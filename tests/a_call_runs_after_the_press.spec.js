import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// `call`: a press that asks the host for an operation - open a document, copy, sign out.
//
// The backend only names it, with its arguments worked out; the page runs it once the answer is
// on screen, and writes what it answers into the call's guarded `into` field, as though typed.

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
import { runCall } from '../src/calls.js'

const base = (name, node_type, parameters = {}, children = [], transparent = false) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: transparent,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

// t / [ said, go ]
const SAID = [0, 0]

const theDocument = () => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('said', 'string', { value: { String: '' }, mutable: { String: 'guarded' } }),
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
  cmd === 'run_overseer_event'
    ? { text: 'TEXT', changes: [], nodes: null, wrote: false, calls }
    : cmd === 'write_overseer_values'
      ? { text: 'TEXT', changes: [], nodes: null, wrote: false }
      : null)

const press = async () => {
  const button = app.currentDocument[0].children[1]
  await app.renderer.emitEvent(
    button,
    document.querySelector(`[data-path='${JSON.stringify(['t', 'go'])}']`),
    'click',
  )
}

// Until the calls the press handed over have run, and what they answered has been written.
const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

const said = () => app.currentDocument[0].children[0].parameters.value
const writes = () => invoke.mock.calls.filter(([cmd]) => cmd === 'write_overseer_values').map(([, args]) => args)

describe('a press that calls', () => {
  let clipboard

  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    clipboard = vi.fn(async () => {})
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: clipboard }, configurable: true })
    delete window.__OVERSEER_HOST__
  })

  afterEach(() => {
    delete window.__OVERSEER_HOST__
  })

  it('runs the operation with the arguments the answer gives', async () => {
    answering([{ name: 'copy', args: { text: 'hello' } }])
    render()
    await press()
    await settled()
    expect(clipboard).toHaveBeenCalledWith('hello')
  })

  it('writes what the operation answers into the field the call names', async () => {
    answering([{ name: 'copy', args: { text: 'hello' }, into: { address: 't/said', path: SAID } }])
    render()
    await press()
    await settled()
    expect(said()).toEqual({ String: 'Copied.' })
    const written = writes()
    expect(written.length, 'the answer was not written').toBe(1)
    expect(written[0].values).toEqual([{ node_path: ['t', 'said'], value: { String: 'Copied.' } }])
  })

  it('says why an operation this host lacks did nothing', async () => {
    window.__OVERSEER_HOST__ = 'browser'
    answering([
      { name: 'open_file', args: {}, into: { address: 't/said', path: SAID } },
      { name: 'copy', args: { text: 'after' } },
    ])
    render()
    await press()
    await settled()
    expect(said()).toEqual({ String: '"open_file" is not available here' })
    // A failed call does not stop the ones after it.
    expect(clipboard).toHaveBeenCalledWith('after')
  })

  it('puts a failure with nowhere to go on the status line', async () => {
    answering([{ name: 'copy', args: { text: 'x' } }])
    clipboard.mockImplementation(async () => { throw new Error('The clipboard is not ours.') })
    render()
    await press()
    await settled()
    expect(document.getElementById('status-message').textContent).toContain('The clipboard is not ours.')
  })
})

describe('an operation', () => {
  it('answers with what it gave', async () => {
    const answer = await runCall({ name: 'here', args: { n: 2 } }, {}, { here: async ({ n }) => n * 2 })
    expect(answer).toEqual({ answer: '4', failed: false })
  })

  it('answers nothing when it gave nothing', async () => {
    expect(await runCall({ name: 'here', args: {} }, {}, { here: async () => {} }))
      .toEqual({ answer: null, failed: false })
  })

  it('that is not known is refused with a sentence', async () => {
    const answer = await runCall({ name: 'launch', args: {} }, {}, {})
    expect(answer).toEqual({ answer: '"launch" is not available here', failed: true })
  })

  it('that fails answers its sentence', async () => {
    const answer = await runCall({ name: 'here', args: {} }, {}, {
      here: async () => { throw new Error('Wrong password.') },
    })
    expect(answer).toEqual({ answer: 'Wrong password.', failed: true })
  })
})
