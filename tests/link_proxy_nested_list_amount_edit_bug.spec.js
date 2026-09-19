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
function findByExactPath(pathArray){
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try {
      const p = JSON.parse(el.dataset.path||'[]')
      if (Array.isArray(p) && p.length===pathArray.length && p.every((v,i)=>v===pathArray[i])) return el
    } catch(_){}
  }
  return null
}

// Build a doc mirroring examples/weight_tracker/weight_tracker_new.os (relevant parts only)
function buildWeightTrackerDoc(){
  return [
    { name:'weight_minimal', node_type:'tab', parameters:{}, is_hierarchy_transparent:false, children:[
      // Selected panel with dynamic link to History keyed by selected_date
      { name:'Selected', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
        { name:'Prev', node_type:'button', parameters:{ label:{ String:'< Prev Day' } }, children:[], is_hierarchy_transparent:false },
        { name:'selected_date', node_type:'timestamp', parameters:{ precision:{ String:'day' }, value:{ Timestamp:'2025.09.08' } }, children:[], is_hierarchy_transparent:false },
        { name:'Next', node_type:'button', parameters:{ label:{ String:'> Next Day' } }, children:[], is_hierarchy_transparent:false },
        // Link proxy with per-link override to unhide intake
        { name:'SelectedWeightRecord', node_type:'div', parameters:{ link:{ String:'/weight_minimal/History[key=$(../selected_date)]' }, 'phantom-materialize':{ String:'prepend-on-edit' } }, is_hierarchy_transparent:false, children:[
          { name:'intake', node_type:'list', parameters:{ hidden:{ Boolean:false } }, children:[], is_hierarchy_transparent:false }
        ] },
      ]},

      // History list with day-precision key and a record for 2025-09-09 that has intake items
      { name:'History', node_type:'list', parameters:{ entry:{ Template:'WeightRecord' }, key:{ String:'date' }, keyPrecision:{ String:'day' } }, is_hierarchy_transparent:false, children:[
        { name:'WeightRecord__1', node_type:'div', parameters:{ _from_template:true, _original_type:'WeightRecord' }, is_hierarchy_transparent:false, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.09.09' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Tuesday' }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
          { name:'weight', node_type:'float', parameters:{ suffix:{ String:' kg' }, precision:{ Integer:1 }, value:{ Float:108.8 } }, children:[], is_hierarchy_transparent:false },
          // Intake list with a MealRecord instance whose 'amount' is inside an unnamed wrapper div
          { name:'intake', node_type:'list', parameters:{ entry:{ Template:'MealRecord' }, hidden:{ Boolean:true }, layout:{ String:'vertical' } }, is_hierarchy_transparent:false, children:[
            { name:'MealRecord__1', node_type:'div', parameters:{ _from_template:true, _original_type:'MealRecord' }, is_hierarchy_transparent:false, children:[
              // Unnamed wrapper (transparent)
              { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
                { name:'description', node_type:'string', parameters:{ value:{ String:'Apple' } }, children:[], is_hierarchy_transparent:false },
                { name:'amount', node_type:'int', parameters:{ label:{ String:'Amount' }, value:{ Integer:1 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
              ]},
              // Additional unnamed wrapper for computed fields (content not needed for this repro)
              { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
                { name:'calories', node_type:'float', parameters:{ label:{ String:'Calories' }, value:{ Float:100 } }, children:[], is_hierarchy_transparent:false },
              ]},
              // per_item and per_100g containers (minimal placeholders)
              { name:'per_item', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
                { name:'calories', node_type:'float', parameters:{ value:{ Float:60 } }, children:[], is_hierarchy_transparent:false },
                { name:'weight', node_type:'float', parameters:{ suffix:{ String:' g' }, value:{ Float:200 } }, children:[], is_hierarchy_transparent:false },
              ]},
              { name:'per_100g', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
                { name:'calories', node_type:'float', parameters:{ value:{ Float:100 } }, children:[], is_hierarchy_transparent:false },
              ]},
            ]}
          ]}
        ]},
        // A couple of other historical records (not strictly necessary for repro)
        { name:'WeightRecord__2', node_type:'div', parameters:{ _from_template:true, _original_type:'WeightRecord' }, is_hierarchy_transparent:false, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.09.07' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'W' } }, children:[], is_hierarchy_transparent:false },
        ]},
      ]}
    ]}
  ]
}

describe('Expected: edit of amount under unnamed wrapper in linked list item updates UI', () => {
  beforeEach(() => setupDOM())

  it('SelectedWeightRecord -> intake[0] -> amount edit reflects new value', async () => {
    const { invoke } = await import('@tauri-apps/api/core')

    let currentDoc = buildWeightTrackerDoc()
    let lastSerialized = null

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      if (cmd === 'serialize_overseer_nodes') { lastSerialized = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('')
      return Promise.reject(new Error('unknown command: '+cmd))
    })

    const app = new OverseerApp()
    // Load and render
    app.currentDocument = deepClone(currentDoc)
    app.renderer.renderDocument(app.currentDocument)

    // Switch selected_date to 2025-09-09 (day precision)
    const selectedPath = ['weight_minimal','Selected','selected_date']
    const selectedNode = app.renderer.findNodeByPath(app.currentDocument, selectedPath)
    expect(selectedNode).toBeTruthy()
    selectedNode.parameters.value = { String:'2025.09.09' }
    app.renderer.renderDocument(app.currentDocument)

    // Find the link container and then the Amount field under it
    const linkEl = findByExactPath(['weight_minimal','Selected','SelectedWeightRecord'])
    expect(linkEl).toBeTruthy()

    // Find element whose data-path ends with '/amount'
    let amountContainer = null
    const all = Array.from(linkEl.querySelectorAll('[data-path]'))
    for (const el of all) {
      try {
        const p = JSON.parse(el.dataset.path||'[]')
        if (Array.isArray(p) && p[p.length-1] === 'amount') { amountContainer = el; break }
      } catch(_){}
    }
    expect(amountContainer).toBeTruthy()
    const holder = amountContainer.querySelector('.field-value, .text-content') || amountContainer
    const originalText = (holder.textContent||'').trim()
    expect(originalText).toBe('1')

    // Trigger edit to change amount to 3
    holder.dispatchEvent(new Event('dblclick', { bubbles:true }))
    const input = linkEl.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeTruthy()
    input.value = '3'
    input.dispatchEvent(new Event('blur'))

    // Allow selective update cycle
    await new Promise(r => setTimeout(r, 0))
    await app.reevaluateDocumentSelective([])
    // Also wait briefly to surface any delayed re-render that might revert the UI
    await new Promise(r => setTimeout(r, 500))

  // Expected: displayed value updated to 3
  const afterText = (holder.textContent||'').trim()
  expect(afterText).toBe('3')

    // Save and verify persisted document contains the updated amount
    await app.saveFile()
    expect(lastSerialized).toBeTruthy()
    const weightRoot = Array.isArray(lastSerialized) ? lastSerialized.find(n=>n.name==='weight_minimal') : lastSerialized
    const history = weightRoot?.children?.find(n=>n.name==='History')
    expect(history).toBeTruthy()
    // Find the 2025-09-09 record
    const record = history.children.find(it => {
      const d = it.children?.find(c=>c.name==='date')?.parameters?.value
      const dv = (d?.String||d||'').toString().replaceAll('-', '.')
      return dv === '2025.09.09'
    })
    expect(record).toBeTruthy()
    const intake = record.children.find(n=>n.name==='intake')
    expect(intake).toBeTruthy()
    const meal = intake.children.find(n => (n.parameters?._original_type||'').toString()==='MealRecord' || (n.name||'').startsWith('MealRecord__'))
    expect(meal).toBeTruthy()
    // Recursively find the 'amount' field under possible unnamed wrappers
    const findByNameDeep = (node, targetName) => {
      if (!node) return null
      if (node.name === targetName) return node
      const kids = Array.isArray(node.children) ? node.children : []
      for (const ch of kids) { const f = findByNameDeep(ch, targetName); if (f) return f }
      return null
    }
    const amountNode = findByNameDeep(meal, 'amount')
    expect(amountNode).toBeTruthy()
    const v = amountNode.parameters?.value
    const iv = (v?.Integer!=null) ? v.Integer : (typeof v === 'string' ? parseInt(v,10) : null)
    expect(iv).toBe(3)
  })
})
