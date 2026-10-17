import { describe, it, expect, beforeEach, vi } from 'vitest'

// A context is what the nodes inside its container see - a theme, a mode - and not one of them. The
// declaration a document writes and the copy the resolver works out from it both reach the page,
// and neither is drawn.

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
import { OverseerApp } from '../src/main.js'

const context = (name, value, copy) => ({
  name, node_type: 'context', children: [], is_hierarchy_transparent: false,
  parameters: Object.assign({ value: { String: value } }, copy ? { _context_copy: { Boolean: true } } : {}),
})

const field = (name, value) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value } },
})

describe('a context', () => {
  beforeEach(() => setupDOM())

  it('is drawn neither as it was written nor as it was worked out', () => {
    const app = new OverseerApp()
    app.currentDocument = [
      context('accent', 'blue-accent', true),
      context('accent', 'blue-accent', false),
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        context('mode', 'dark-mode', true),
        context('mode', 'dark-mode', false),
        field('shown', 'what is drawn'),
      ] },
    ]
    app.renderer.renderDocument(app.currentDocument)
    const page = document.body.textContent
    expect(page).toContain('what is drawn')
    expect(page).not.toContain('blue-accent')
    expect(page).not.toContain('dark-mode')
    expect(document.querySelector(`[data-path='${JSON.stringify(['t', 'mode'])}']`)).toBeNull()
  })
})
