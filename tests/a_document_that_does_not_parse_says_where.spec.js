import { describe, it, expect, beforeEach, vi } from 'vitest'

// What the parser could not read is said on the page, and a field it left without a value is
// drawn empty.
//
// The parser leaves `_parse_error` on the node nearest the text it passed over. The page draws
// it as it draws a template that cannot be had, and the status line does not call the document
// well. A field with no value drew the id the page gives every node, `uid_...`, as its value.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const why = (text) => ({ _parse_error: { String: text } })

// What the parser makes of the item's example: three nodes, none with a value.
const unreadable = () => [
  base('Page', 'div', why('The { on line 1 is never closed')),
  base('name', 'string', why('The text on line 2 has no closing quote: string name = "A page')),
  base('x', 'int', why('Could not read line 3: int x =')),
]

function open(doc) {
  setupDOM()
  const app = new OverseerApp()
  app.currentDocument = doc
  app.currentFile = 'page.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  app.updateTitle()
  return app
}

describe('a document that does not parse', () => {
  let app
  beforeEach(() => { app = open(unreadable()) })

  it('says each line it could not read', () => {
    const lines = [...document.querySelectorAll('.inherit-error')].map((l) => l.textContent)
    expect(lines).toEqual([
      'The { on line 1 is never closed',
      'The text on line 2 has no closing quote: string name = "A page',
      'Could not read line 3: int x =',
    ])
  })

  it('draws no page ids', () => {
    expect(document.getElementById('content-display').textContent).not.toMatch(/uid_/)
  })

  it('does not call itself saved and well', () => {
    const status = document.getElementById('status-message')
    expect(status.classList.contains('warning')).toBe(true)
    expect(document.getElementById('status-info').textContent).not.toMatch(/All changes saved/)
  })
})

describe('a document that parses', () => {
  it('is saved and well', () => {
    open([base('name', 'string', { value: { String: 'A page' } })])
    expect(document.querySelector('.inherit-error')).toBeNull()
    expect(document.getElementById('status-info').textContent).toBe('All changes saved')
  })
})

describe('a field without a value', () => {
  it('draws nothing as its value', () => {
    // As the server sends it: a node with no children has no `children` at all.
    const bare = (name, node_type) => ({ name, node_type, parameters: { _label_layout: { String: 'horizontal' } } })
    open([bare('name', 'string'), bare('x', 'int')])
    expect(document.getElementById('content-display').textContent).not.toMatch(/uid_/)
  })
})
