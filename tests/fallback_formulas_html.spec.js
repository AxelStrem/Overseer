import { describe, it, expect, beforeEach, vi } from 'vitest'

// Minimal DOM container expected by renderer
function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
        <button id="save-file-btn"></button>
        <button id="reload-file-btn"></button>
      </div>
      <button id="welcome-open-btn"></button>
      <button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div>
  <div id="main-content"></div>
      <div id="content-display"></div>
      <div id="status-bar">
        <span id="status-message"></span>
        <span id="status-info"></span>
        <span id="file-path"></span>
      </div>
      <div id="welcome-screen" class="screen"></div>
      <div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div>
      <div id="error-message"></div>
    </div>
  `
}

// Stub Tauri invoke to avoid noisy errors in jsdom
vi.mock('@tauri-apps/api/tauri', () => ({ invoke: vi.fn(() => Promise.resolve(null)) }))

import { OverseerApp } from '../src/main.js'

describe('fallback_formulas.os HTML output', () => {
  beforeEach(() => setupDOM())

  it('renders 10 for int v(fallback=10) = null', async () => {
    const app = new OverseerApp()

    // Document identical to examples/basic/fallback_formulas.os
    const doc = [
      {
        name: 'Root',
        node_type: 'div',
        parameters: {},
        children: [
          { name: 'v', node_type: 'int', parameters: { value: { Null: null }, fallback: { Integer: 10 }, mutable: { Boolean: true } }, children: [] }
        ]
      }
    ]

    app.currentDocument = doc
    app.renderer.renderDocument(doc)

  const fieldEl = document.querySelector('.number-field .field-value')
  expect(fieldEl).toBeTruthy()
  expect(fieldEl.textContent.trim()).toBe('10')

  // Assert the user-facing area (#main-content or #content-display) has the value and no 'Null'
  const html = (document.getElementById('main-content')?.innerHTML || '') + document.getElementById('content-display').innerHTML
    expect(html).toContain('10')
    expect(/null/i.test(html)).toBe(false)
  })

  it('edit box shows empty when value is null and fallback displays 10', async () => {
    const app = new OverseerApp()
    const doc = [
      {
        name: 'Root',
        node_type: 'div',
        parameters: {},
        children: [
          { name: 'v', node_type: 'int', parameters: { value: { Null: null }, fallback: { Integer: 10 }, mutable: { Boolean: true } }, children: [] }
        ]
      }
    ]
    app.currentDocument = doc
    app.renderer.renderDocument(doc)

    const valueEl = document.querySelector('.number-field .field-value')
    expect(valueEl.textContent.trim()).toBe('10')
  // Enter edit mode (use MouseEvent with bubbles for jsdom)
  valueEl.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    const input = valueEl.parentElement.querySelector('input.field-editor')
    expect(input).toBeTruthy()
    expect(input.value).toBe('')
  })

  it('treats computed_value string "Null" as empty and uses fallback', async () => {
    const app = new OverseerApp()

    // Simulate a backend that returns the literal string "Null" for _computed_value
    const doc = [
      {
        name: 'Root',
        node_type: 'div',
        parameters: {},
        children: [
          { name: 'v', node_type: 'int', parameters: { value: { Null: null }, _computed_value: 'Null', _computed_fallback: { Integer: 10 } }, children: [] }
        ]
      }
    ]

    app.currentDocument = doc
    app.renderer.renderDocument(doc)

    const fieldEl = document.querySelector('.number-field .field-value')
    expect(fieldEl).toBeTruthy()
    expect(fieldEl.textContent.trim()).toBe('10')
  const html = (document.getElementById('main-content')?.innerHTML || '') + document.getElementById('content-display').innerHTML
    expect(html).toContain('10')
    expect(/null/i.test(html)).toBe(false)
  })

  it('treats node.value string "Null" as empty and uses fallback', async () => {
    const app = new OverseerApp()

    // Simulate a backend that sets node.value to the literal string "Null"
    const doc = [
      {
        name: 'Root',
        node_type: 'div',
        parameters: {},
        children: [
          { name: 'v', node_type: 'int', value: 'Null', parameters: { fallback: { Integer: 10 } }, children: [] }
        ]
      }
    ]

    app.currentDocument = doc
    app.renderer.renderDocument(doc)

    const fieldEl = document.querySelector('.number-field .field-value')
    expect(fieldEl).toBeTruthy()
    // Expect fallback to be shown (this currently fails prior to fix)
    expect(fieldEl.textContent.trim()).toBe('10')
    const html = (document.getElementById('main-content')?.innerHTML || '') + document.getElementById('content-display').innerHTML
    expect(html).toContain('10')
    expect(/null/i.test(html)).toBe(false)
  })

  it('treats parameters.value string "Null" as empty and uses fallback', async () => {
    const app = new OverseerApp()

    // Simulate parameters.value being the string "Null"
    const doc = [
      {
        name: 'Root',
        node_type: 'div',
        parameters: {},
        children: [
          { name: 'v', node_type: 'int', parameters: { value: 'Null', fallback: { Integer: 10 } }, children: [] }
        ]
      }
    ]

    app.currentDocument = doc
    app.renderer.renderDocument(doc)

    const fieldEl = document.querySelector('.number-field .field-value')
    expect(fieldEl).toBeTruthy()
    expect(fieldEl.textContent.trim()).toBe('10')
    const html = (document.getElementById('main-content')?.innerHTML || '') + document.getElementById('content-display').innerHTML
    expect(html).toContain('10')
    expect(/null/i.test(html)).toBe(false)
  })
})
