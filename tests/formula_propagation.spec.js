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

// Mock tauri before imports for this file scope
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}))

import { OverseerRenderer } from '../src/renderer.js'
import { OverseerApp } from '../src/main.js'

function setupApp(renderer, doc) {
  global.window.app = {
    currentDocument: doc,
    renderer,
    markDocumentModified: () => {},
    startScheduler: () => {},
  }
}

// Helper to locate an element by logical path
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

describe('Formula propagation', () => {
  beforeEach(() => setupDOM())

  it('does not display raw formula text for computed values', () => {
    const renderer = new OverseerRenderer()
    const node = {
      name: 'g', node_type: 'int', parameters: {
        value: { Formula: 'f*2' },
        _computed_value: { Formula: 'f*2' },
      }, children: []
    }
    // getNodeValue should not surface the formula literal
    const v = renderer.getNodeValue(node)
    expect(v).not.toBe('f*2')
  })

  it('propagates dependent value after source change (g = f*2)', async () => {
    // Mock tauri invoke to emulate backend
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content_selective') {
        // Return a doc with only f updated to 5, g still computed=6 (to emulate partial selective update)
        const doc = deepClone(currentDoc)
        const root = doc[0]
        root.children.find(n => n.name === 'f').parameters.value = { Integer: 5 }
        // g unchanged here
        return Promise.resolve(doc)
      }
      if (cmd === 'parse_overseer_content') {
        // Full resolve recomputes g to 10
        const doc = deepClone(currentDoc)
        const root = doc[0]
        root.children.find(n => n.name === 'f').parameters.value = { Integer: 5 }
        root.children.find(n => n.name === 'g').parameters._computed_value = { Integer: 10 }
        return Promise.resolve(doc)
      }
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    // Now import app after mock in effect
    const app = new OverseerApp()

    // Initial document identical to examples/basic/formula_propagation.os
    const before = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'f', node_type: 'int', parameters: { value: { Integer: 3 } }, children: [] },
        { name: 'g', node_type: 'int', parameters: { value: { Formula: 'f*2' }, _computed_value: { Integer: 6 } }, children: [] },
      ]}
    ]
    currentDoc = before
    app.currentDocument = before
    app.renderer.renderDocument(before)

    // Before edit
  let fEl = findElementByPath(['Root','f'])
  let gEl = findElementByPath(['Root','g'])
    expect((fEl.querySelector('.field-value') || fEl).textContent).toContain('3')
    expect((gEl.querySelector('.field-value') || gEl).textContent).toContain('6')

    // Emulate user change of f -> 5, trigger selective reevaluation through app
    await app.reevaluateDocumentSelective(['Root/f'], [{ path: 'Root/f', oldValue: '3', newValue: '5' }])
    // Update currentDoc used by mocks to the app's current document for next phases
    currentDoc = app.currentDocument

  // Re-query after potential rerender
  fEl = findElementByPath(['Root','f'])
  gEl = findElementByPath(['Root','g'])
  // Expect both f and g rendered with 5 and 10
    expect((fEl.querySelector('.field-value') || fEl).textContent).toContain('5')
    expect((gEl.querySelector('.field-value') || gEl).textContent).toContain('10')
  // And underlying document should reflect computed 10 as well
  const gNode = app.currentDocument[0].children.find(n => n.name === 'g')
  expect(gNode.parameters._computed_value.Integer).toBe(10)
  })
})
