import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A document's `on tick` is run by the page's clock, as often as the node's `tick=` says, for as
// long as the document is open - and never two at once. A stopped clock starts when `tick=`
// says so, and a refusal stops it and is said once.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const aDocument = (tick, withOnTick = true) => [base('clock', 'tab', { mutable: { Boolean: true }, ...(tick === undefined ? {} : { _computed_tick: tick }) }, [
  base('beats', 'int', { value: { Integer: 0 }, label: { String: 'Beats' } }),
  ...(withOnTick ? [base('tick', 'on', {}, [base('', 'inc', { path: { String: '/clock/beats' } })])] : []),
])]

// What the server answers to a tick: the guarded field, stepped for this viewer.
const aBeat = (n) => ({
  version: 'v1',
  changes: [{ kind: 'parameters', path: [0, 0], parameters: { value: { Integer: n }, label: { String: 'Beats' } } }],
  nodes: null,
  wrote: false,
})

const ticks = () => invoke.mock.calls.filter(([cmd]) => cmd === 'run_overseer_tick')

const opening = (doc, onTick) => invoke.mockImplementation(async (cmd, args) => {
  if (cmd === 'load_overseer_file') return 'tab clock {}\n'
  if (cmd === 'parse_overseer_content') return doc
  if (cmd === 'run_overseer_tick') return onTick(args)
  return null
})

const shownBeats = () =>
  document.querySelector(`[data-path='${JSON.stringify(['clock', 'beats'])}'] .field-value`)?.textContent

describe('a document ticks while it is open', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('ticks as often as tick= says, and shows what each tick set', async () => {
    const app = new OverseerApp()
    let n = 0
    opening(aDocument({ String: '500ms' }), () => aBeat(++n))
    await app.loadFile('clock.os')
    expect(ticks()).toHaveLength(0)
    await vi.advanceTimersByTimeAsync(500)
    expect(ticks()).toHaveLength(1)
    expect(ticks()[0][1]).toMatchObject({ path: 'clock.os', owners: ['clock'] })
    await vi.advanceTimersByTimeAsync(1000)
    expect(ticks()).toHaveLength(3)
    expect(shownBeats()).toBe('3')
    app.stopTheClock()
  })

  it('does not tick without on tick, or while tick= is nought', async () => {
    const app = new OverseerApp()
    opening(aDocument({ Integer: 0 }), () => aBeat(1))
    await app.loadFile('clock.os')
    await vi.advanceTimersByTimeAsync(3000)
    expect(ticks()).toHaveLength(0)

    opening(aDocument({ Integer: 1 }, false), () => aBeat(1))
    await app.loadFile('clock.os')
    await vi.advanceTimersByTimeAsync(3000)
    expect(ticks()).toHaveLength(0)
  })

  it('starts when tick= comes to say how often', async () => {
    const app = new OverseerApp()
    opening(aDocument({ Integer: 0 }), () => aBeat(1))
    await app.loadFile('clock.os')
    await vi.advanceTimersByTimeAsync(1000)
    expect(ticks()).toHaveLength(0)
    app.currentDocument[0].parameters._computed_tick = { Integer: 1 }
    await vi.advanceTimersByTimeAsync(1300)
    expect(ticks()).toHaveLength(1)
    app.stopTheClock()
  })

  it('never has two ticks on their way at once', async () => {
    const app = new OverseerApp()
    let answer
    opening(aDocument({ String: '100ms' }), () => new Promise((resolve) => { answer = resolve }))
    await app.loadFile('clock.os')
    await vi.advanceTimersByTimeAsync(1000)
    expect(ticks()).toHaveLength(1)
    answer(aBeat(1))
    await vi.advanceTimersByTimeAsync(100)
    expect(ticks()).toHaveLength(2)
    app.stopTheClock()
  })

  it('stops when another document is opened', async () => {
    const app = new OverseerApp()
    opening(aDocument({ Integer: 1 }), () => aBeat(1))
    await app.loadFile('clock.os')
    opening(aDocument(undefined, false), () => aBeat(1))
    await app.loadFile('other.os')
    await vi.advanceTimersByTimeAsync(3000)
    expect(ticks()).toHaveLength(0)
  })

  it('says a refusal once, and stops', async () => {
    const app = new OverseerApp()
    opening(aDocument({ Integer: 1 }), () => { throw new Error('on tick may write only guarded fields') })
    await app.loadFile('clock.os')
    await vi.advanceTimersByTimeAsync(5000)
    expect(ticks()).toHaveLength(1)
    expect(document.getElementById('status-message').textContent).toBe('on tick did not run')
  })
})
