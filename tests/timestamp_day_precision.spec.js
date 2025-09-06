import { describe, it, expect, beforeEach, vi } from 'vitest'

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

describe('Timestamp precision day display', () => {
  beforeEach(() => setupDOM())

  it('renders timestamp with precision="day" as date-only and updates after click', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/tauri')

    // Fake backend: resolve and event execution
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') {
        return Promise.resolve(deepClone(currentDoc))
      }
      if (cmd === 'parse_overseer_content_selective') {
        return Promise.resolve(deepClone(currentDoc))
      }
      if (cmd === 'execute_overseer_event') {
        // Simulate ActionExecutor::execute_event for a click on Prev: decrement day
        const doc = deepClone(args.nodes)
        const tab = doc[0]
        const selected = tab.children.find(n => n.name === 'Selected')
        const field = selected.children.find(n => n.name === 'selected_date')
    // Deterministic decrement: set to a fixed one-day-earlier ISO instead of relying on timezone math
    field.parameters.value = { Timestamp: '2025-01-14T12:00:00Z' }
        return Promise.resolve(doc)
      }
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    // Minimal doc with a timestamp field and a button triggering click
  const todayIso = '2025-01-15T12:00:00Z'
    currentDoc = [
      { name: 'tab1', node_type: 'tab', parameters: {}, children: [
        { name: 'Selected', node_type: 'div', parameters: {}, children: [
          { name: 'selected_date', node_type: 'timestamp', parameters: { value: { Timestamp: todayIso }, precision: { String: 'day' } }, children: [] },
          { name: 'Prev', node_type: 'button', parameters: { label: { String: '< Prev Day' } }, children: [
            { name: 'click', node_type: 'on', parameters: {}, children: [] }
          ] }
        ] }
      ]}
    ]

    const app = new OverseerApp()
    app.currentDocument = currentDoc
    app.renderer.renderDocument(currentDoc)

    const fieldEl = findElementByPath(['tab1','Selected','selected_date'])
    let text = (fieldEl.querySelector('.field-value') || fieldEl).textContent.trim()
    // Expect YYYY-MM-DD
    expect(/\d{4}-\d{2}-\d{2}$/.test(text)).toBe(true)

  const btnEl = findElementByPath(['tab1','Selected','Prev'])
  btnEl.click()
  // Wait for async emitEvent -> invoke -> renderDocument cycle to finish
  await Promise.resolve()
  await new Promise((r) => setTimeout(r, 0))

    // After click, renderer re-renders; check text again
  const fieldEl2 = findElementByPath(['tab1','Selected','selected_date'])
    const text2 = (fieldEl2.querySelector('.field-value') || fieldEl2).textContent.trim()
    expect(/\d{4}-\d{2}-\d{2}$/.test(text2)).toBe(true)
    expect(text2).not.toEqual(text)
  })
})
