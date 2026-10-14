import { describe, it, expect, beforeEach, vi } from 'vitest'

// A field of text emptied in the page holds empty text, not null.
//
// Taking the last chip off a tags field sent Null to the server, which wrote `- flags = null`
// into the project tracker; tracker.py read the word as a flag, and the next `ask` made it
// "null, asked". A number or a date emptied still holds nothing.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button></div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

const field = (node_type) => ({ name: 'f', node_type, parameters: { value: { String: 'auto' } }, children: [] })

describe('a field emptied in the page', () => {
  let app
  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
  })

  it('sends empty text for a field of text', () => {
    for (const kind of ['tags', 'string', 'text', 'textbox', 'enum']) {
      expect(app.coerceToOverseerValue(field(kind), ''), kind).toEqual({ String: '' })
    }
  })

  it('holds empty text for a field of text', () => {
    for (const kind of ['tags', 'string', 'text', 'textbox', 'enum']) {
      const node = field(kind)
      app.renderer.updateNodeValue(node, '')
      expect(node.parameters.value, kind).toEqual({ String: '' })
    }
  })

  it('still holds nothing for a number or a date', () => {
    for (const kind of ['int', 'float', 'date', 'timestamp']) {
      expect(app.coerceToOverseerValue(field(kind), ''), kind).toEqual({ Null: null })
      const node = field(kind)
      app.renderer.updateNodeValue(node, '')
      expect(node.parameters.value, kind).toEqual({ Null: null })
    }
  })
})
