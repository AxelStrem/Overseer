import { describe, it, expect, beforeEach, vi } from 'vitest'

// A page names the text it shows with every change it sends.
//
// The answer to a change is a list of changes applied by child index to the tree the page holds,
// so the backend has to describe it against that tree. It described it against whatever it held
// for the file's text, which after the bot wrote through the same server was the bot's document:
// the page's change was applied to a tree without the bot's entry, and the page showed a list
// that was not the file's until it was reloaded. Now the page says which text it is showing, and
// a page behind the file is answered with the whole document.

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

const TEXT = 'tab day (label="Day", mutable=true) {\n    int n = 1\n}\n'

const field = (name, n) => ({ name, node_type: 'int', parameters: { value: { Integer: n } }, children: [] })

const anApp = () => {
  const app = new OverseerApp()
  app.currentFile = '/documents/day.os'
  app._currentText = TEXT
  app._originalText = TEXT
  app.currentDocument = [{ name: 'day', node_type: 'tab', parameters: {}, children: [field('n', 1)] }]
  return app
}

const writes = () => invoke.mock.calls.filter(([cmd]) => cmd === 'write_overseer_values')

describe('a page names the text it shows', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
  })

  it('works a version out as the backend does', () => {
    // The same text and the same value as `a_version_is_the_same_on_both_sides` in
    // `an_answer_is_described_against_what_the_caller_holds.rs`, so the two cannot drift.
    expect(versionOf('tab t (label="Día ✓") {\n    int n = 1\n}\n')).toBe('1a551fd9d4756f61-43')
    expect(versionOf('')).toBe('cbf29ce484222325-0')
  })

  it('names the text it holds, or the version it was handed', () => {
    const app = anApp()
    expect(app.showing()).toEqual({ shown_version: versionOf(TEXT), shownVersion: versionOf(TEXT) })
    app.tookTheText({ version: 'abc-1' })
    expect(app.showing()).toEqual({ shown_version: 'abc-1', shownVersion: 'abc-1' })
    app.tookTheText({})
    expect(app.showing(), 'a page holding nothing named something').toEqual({})
  })

  it('sends it with a field write', async () => {
    const app = anApp()
    invoke.mockImplementation((cmd) =>
      cmd === 'write_overseer_values' ? Promise.resolve({ version: 'v2', changes: [], nodes: null }) : Promise.resolve(null))
    await app.reevaluateDocumentSelective(['day/n'], [{ path: 'day/n', oldValue: 1, newValue: 2 }])
    expect(writes()).toHaveLength(1)
    expect(writes()[0][1].shown_version).toBe(versionOf(TEXT))
  })

  it('takes a whole document when it was behind, and names that next', async () => {
    // What the backend answers a page that is behind the file: everything, the other writer's
    // entry included, rather than changes the page would apply to the wrong tree.
    const app = anApp()
    const NOW = TEXT.replace('    int n = 1\n', '    int n = 2\n    int m = 5\n')
    invoke.mockImplementation((cmd) =>
      cmd === 'write_overseer_values'
        ? Promise.resolve({
            text: NOW, version: versionOf(NOW), changes: null, wrote: true,
            nodes: [{ name: 'day', node_type: 'tab', parameters: {}, children: [field('n', 2), field('m', 5)] }],
          })
        : Promise.resolve(null))
    await app.reevaluateDocumentSelective(['day/n'], [{ path: 'day/n', oldValue: 1, newValue: 2 }])
    const names = app.currentDocument[0].children.map((c) => c.name)
    expect(names, 'the other writer\'s field never reached the page').toEqual(['n', 'm'])
    expect(app.showing().shown_version).toBe(versionOf(NOW))
  })
})
