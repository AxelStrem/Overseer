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

describe('Edit persistence inside a nested list under link proxy', () => {
  beforeEach(() => setupDOM())

  it('keeps edited value after selective update without reverting', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    // Backend stubs: round-trip the current in-memory document
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

    // Document: Data has a list with an item; View links into Data so edits flow through the proxy
    currentDoc = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'Data', node_type: 'div', parameters: {}, children: [
          { name: 'Day', node_type: 'div', parameters: {}, children: [
            { name: 'Intake', node_type: 'list', parameters: { entry: { Template: 'Meal' } }, children: [
              { name: 'Meal__1', node_type: 'Meal', parameters: { _from_template: { Boolean: true } }, children: [
                { name: 'amount', node_type: 'int', parameters: { value: { Integer: 1 } }, children: [] }
              ] }
            ] }
          ] }
        ] },
        { name: 'View', node_type: 'div', parameters: { link: '/Root/Data/Day' }, children: [] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

    // Sanity: proxy renders Day subtree
  const amountEl = findElementByPath(['Root','Data','Day','Intake','Meal__1','amount'])
    expect(amountEl).toBeTruthy()

    // Simulate editing amount from 1 -> 2 via API used by inline editor
  const fieldPath = 'Root/Data/Day/Intake/Meal__1/amount'
    renderer.updateNodeValueByPath(app.currentDocument, fieldPath, 2)
    // Run selective update (this used to re-render link proxy and could overwrite the change)
    await app.reevaluateDocumentSelective([fieldPath], [{ path: fieldPath, oldValue: 1, newValue: 2 }])

    // Verify value persisted in the underlying doc
    const day = app.currentDocument[0].children.find(n=>n.name==='Data').children.find(n=>n.name==='Day')
    const intake = day.children.find(n=>n.name==='Intake')
    const meal = intake.children[0]
    const amountNode = meal.children.find(n=>n.name==='amount')
    expect(amountNode.parameters.value.Integer).toBe(2)

    // Also verify the DOM reflects the change (no revert)
  const updatedEl = findElementByPath(['Root','Data','Day','Intake','Meal__1','amount'])
    expect(updatedEl).toBeTruthy()
  const holder = updatedEl.querySelector('.field-value, .text-content, .overseer-list-value, span, div') || updatedEl
  expect((holder.textContent || '').trim()).toBe('2')
  })
})
