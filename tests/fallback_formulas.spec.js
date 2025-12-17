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

vi.mock('@tauri-apps/api/tauri', () => ({ invoke: vi.fn() }))

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

describe('Fallback formulas rendering', () => {
  beforeEach(() => setupDOM())

  it('renders numeric fallback when value is explicit null', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/tauri')
    // Backend stubs: first full load returns computed fallback=10 for node v
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') {
        // Resolve fallback into _computed_fallback since raw value is Null
        const doc = deepClone(currentDoc)
        const root = doc[0]
        const v = root.children.find(n => n.name === 'v')
        v.parameters._computed_fallback = { Integer: 10 }
        v.parameters._computed_value = { Null: null }
        return Promise.resolve(doc)
      }
      if (cmd === 'parse_overseer_content_selective') {
        // No change on selective for this test
        return Promise.resolve(currentDoc)
      }
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()

    // Document mirrors examples/basic/fallback_formulas.os
    const initial = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'v', node_type: 'int', parameters: { value: { Null: null }, fallback: { Integer: 10 } }, children: [] }
      ]}
    ]
    currentDoc = initial

    app.currentDocument = initial
    app.renderer.renderDocument(initial)

    const vEl = findElementByPath(['Root','v'])
    // Renderer formats numbers; expect '10'
    expect((vEl.querySelector('.field-value') || vEl).textContent).toContain('10')
  })
})
