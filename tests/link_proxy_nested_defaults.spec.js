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

function deepClone(o){ return JSON.parse(JSON.stringify(o)) }

// Minimal doc to exercise list templating with a named container and default fields
function buildDoc(){
  return [
    { name:'Root', node_type:'tab', parameters:{}, children:[
      // Template declaration, hidden
      { name:'MealRecord', node_type:'div', parameters:{ hidden:{ Boolean: true } }, children:[
        { name:'per_item', node_type:'div', parameters:{}, children:[
          { name:'calories', node_type:'float', parameters:{ value:{ Float: 0.0 } }, children:[], is_hierarchy_transparent:false },
          { name:'weight', node_type:'float', parameters:{ value:{ Float: 100.0 } }, children:[], is_hierarchy_transparent:false }
        ], is_hierarchy_transparent:false }
      ], is_hierarchy_transparent:false },
      // Resolved list with one instantiated item from the template
      { name:'Intake', node_type:'list', parameters:{ entry:{ Template:'MealRecord' } }, children:[
        { name:'MealRecord__1', node_type:'MealRecord', parameters:{ _from_template:{ Boolean: true } }, children:[
          { name:'per_item', node_type:'div', parameters:{}, children:[
            { name:'calories', node_type:'float', parameters:{ value:{ Float: 250.0 } }, children:[], is_hierarchy_transparent:false },
            { name:'weight', node_type:'float', parameters:{ value:{ Float: 100.0 } }, children:[], is_hierarchy_transparent:false }
          ], is_hierarchy_transparent:false }
        ], is_hierarchy_transparent:false }
      ], is_hierarchy_transparent:false }
    ], is_hierarchy_transparent:false }
  ]
}

describe('nested named container defaults inside list item templates', () => {
  beforeEach(() => setupDOM())

  it('preserves defaults for sibling fields when overriding one child', async () => {
    const { invoke } = await import('@tauri-apps/api/core')

    let currentDoc = buildDoc()

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('')
      return Promise.reject(new Error('unknown command: '+cmd))
    })

    const app = new OverseerApp()
    app.currentDocument = deepClone(currentDoc)
    app.renderer.renderDocument(app.currentDocument)

    // After render, the list item should be resolved into a MealRecord with per_item children
    const root = app.currentDocument.find(n=>n.name==='Root')
    const intake = root.children.find(n=>n.name==='Intake')
    expect(intake.children.length).toBe(1)
    const item = intake.children[0]
    expect(item.node_type).toBe('MealRecord')

    const perItem = item.children.find(n=>n.name==='per_item')
    const calories = perItem.children.find(n=>n.name==='calories')
    const weight = perItem.children.find(n=>n.name==='weight')

    // calories overridden to 250.0; weight should remain default 100.0 from template
    const calVal = calories.parameters?.value?.Float ?? calories.parameters?.value
    const wtVal = weight.parameters?.value?.Float ?? weight.parameters?.value
    expect(calVal).toBe(250.0)
    expect(wtVal).toBe(100.0)
  })
})
