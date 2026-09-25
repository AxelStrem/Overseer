import { describe, it, expect, beforeEach, vi } from 'vitest'

// A field can say how heavy its text is, from a formula as readily as a size - which is how a task
// with things under it reads in bold in the project documents. Only a weight CSS knows is applied,
// so a formula not yet worked out is never written into the style.

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

const title = (name, weight) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false,
  parameters: Object.assign({ value: { String: name } }, weight),
})

const render = (...fields) => {
  const app = new OverseerApp()
  app.currentDocument = [{ name: 't', node_type: 'tab', parameters: {}, children: fields, is_hierarchy_transparent: false }]
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const weightOf = (name) =>
  document.querySelector(`[data-path='${JSON.stringify(['t', name])}'] .field-value`).style.fontWeight

describe('a field’s weight', () => {
  beforeEach(() => setupDOM())

  it('is bold when a formula works out to bold, and normal when it works out to normal', () => {
    const formula = { Formula: '../kids > 0 ? "bold" : "normal"' }
    render(
      title('parent', { 'font-weight': formula, '_computed_font-weight': { String: 'bold' } }),
      title('leaf', { 'font-weight': formula, '_computed_font-weight': { String: 'normal' } }),
    )
    expect(weightOf('parent')).toBe('bold')
    expect(weightOf('leaf')).toBe('normal')
  })

  it('takes a number, as CSS does', () => {
    render(title('heavy', { 'font-weight': { Integer: 700 } }))
    expect(weightOf('heavy')).toBe('700')
  })

  it('is left alone while its formula has not been worked out', () => {
    render(title('pending', { 'font-weight': { Formula: '../kids > 0 ? "bold" : "normal"' } }))
    expect(weightOf('pending')).toBe('')
  })
})
