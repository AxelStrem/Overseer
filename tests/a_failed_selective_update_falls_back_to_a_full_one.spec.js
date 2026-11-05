import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

// When the selective update fails, the page works the whole document out again from its text
// and redraws it. That fallback used to fail every time: it checked whether a newer request had
// overtaken it with a ticket declared inside the `try` it was catching, so reading it threw a
// ReferenceError, "Fallback update also failed" was logged, and the page kept what it showed.

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

const text = 'tab day (label="Day") {\n    int n = 2\n}\n'

const aDay = (n) => [{
  name: 'day', node_type: 'tab', parameters: { label: { String: 'Day' } }, is_hierarchy_transparent: false,
  children: [
    { name: 'n', node_type: 'int', parameters: { value: { Integer: n } }, children: [], is_hierarchy_transparent: false },
  ],
}]

describe('a failed selective update falls back to a full one', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation(async (cmd) => (cmd === 'parse_overseer_content' ? aDay(2) : null))
  })
  afterEach(() => { vi.restoreAllMocks() })

  it('works the document out from its text and redraws it', async () => {
    const app = new OverseerApp()
    app.currentFile = '/documents/day.os'
    app.currentDocument = aDay(1)
    // The selective way finds no text to send and gives up; the fallback, asking next, has it.
    app.textToWorkFrom = vi.fn().mockResolvedValueOnce(null).mockResolvedValue(text)
    const drawn = vi.spyOn(app.renderer, 'renderDocument').mockImplementation(() => {})
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {})
    vi.spyOn(console, 'warn').mockImplementation(() => {})

    const result = await app.reevaluateDocumentSelective([], [])

    expect(errors.mock.calls.map((c) => String(c[0])).filter((m) => m.includes('Fallback update also failed')),
      'the fallback failed too').toEqual([])
    expect(result).toEqual({ domOnly: false, success: true })
    expect(invoke.mock.calls.map(([cmd]) => cmd)).toContain('parse_overseer_content')
    expect(app.currentDocument[0].children[0].parameters.value).toEqual({ Integer: 2 })
    expect(drawn).toHaveBeenCalled()
  })
})
