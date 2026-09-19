import { describe, it, expect, beforeEach, vi } from 'vitest'

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
      </div>
      <button id="welcome-open-btn"></button>
      <button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div>
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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

import { OverseerRenderer } from '../src/renderer.js'
import { OverseerApp } from '../src/main.js'

function findElementByPath(pathArray) {
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try {
      const p = JSON.parse(el.dataset.path || '[]')
      if (Array.isArray(p) && p.length === pathArray.length && p.every((v,i)=>v===pathArray[i])) return el
    } catch(_) {}
  }
  return null
}

describe('Dynamic link with $(...) inside selector', () => {
  beforeEach(() => setupDOM())

  it('does not break on \'/\' inside $(...) and renders phantom preview (create-on-edit ready)', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    // Build minimal weight_minimal structure with Selected/selected_date and a link using $(../selected_date)
    currentDoc = [
      { name: 'WeightRecord', node_type: 'div', parameters: {}, children: [
  { name: 'date', node_type: 'timestamp', parameters: { value: { Timestamp: '2025-09-06T00:00:00Z' }, precision: { String: 'day' } }, children: [] },
  { name: 'test_data', node_type: 'string', parameters: { value: { String: '' }, mutable: { Boolean: true } }, children: [] }
      ]},
      { name: 'weight_minimal', node_type: 'tab', parameters: {}, children: [
        { name: 'Selected', node_type: 'div', parameters: {}, children: [
          { name: 'selected_date', node_type: 'timestamp', parameters: { value: { Timestamp: '2025-09-07T00:00:00Z' }, precision: { String: 'day' } }, children: [] },
          { name: 'SelectedWeightRecord', node_type: 'div', parameters: { link: { String: '/weight_minimal/History[key=$(../selected_date)]' } }, children: [] }
        ]},
        { name: 'History', node_type: 'list', parameters: { entry: { Template: 'WeightRecord' }, key: { String: 'date' }, keyPrecision: { String: 'day' } }, children: [] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

    const linkContainer = findElementByPath(['weight_minimal','Selected','SelectedWeightRecord'])
    expect(linkContainer).toBeTruthy()
    // Should not show broken link placeholder
    const placeholder = linkContainer.querySelector('.overseer-link-placeholder')
    expect(placeholder).toBeFalsy()
    // Should render as phantom until item exists
    expect(linkContainer.hasAttribute('data-link-phantom')).toBe(true)

    // Edit through phantom to create the item
  // Pick an editable field (string) rather than the non-editable timestamp
  const valueEl = linkContainer.querySelector('.string-field .field-value') || linkContainer.querySelector('.field-value')
    expect(valueEl).toBeTruthy()
    valueEl.dispatchEvent(new Event('dblclick', { bubbles: true }))
    const input = linkContainer.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeTruthy()
    input.value = 'New Test'
    input.dispatchEvent(new Event('blur'))
    await new Promise(r => setTimeout(r, 0))
    await app.reevaluateDocumentSelective([])

    const root = app.currentDocument.find(n=>n.name==='weight_minimal')
    const hist = root.children.find(n=>n.name==='History')
    expect(Array.isArray(hist.children)).toBe(true)
    const created = hist.children.find(it => it.children.some(f => f.name==='date' && (f.parameters.value?.Timestamp || f.parameters.value?.String || f.parameters.value) ))
    expect(created).toBeTruthy()
  })
})
