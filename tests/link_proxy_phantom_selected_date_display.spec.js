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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

import { OverseerApp } from '../src/main.js'

function deepClone(o){ return JSON.parse(JSON.stringify(o)) }
function findByPath(pathArray){
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p.length===pathArray.length && p.every((v,i)=>v===pathArray[i])) return el } catch(_){}}
  return null
}

// Build a JS doc equivalent to examples/weight_tracker/weight_tracker_new.os
function buildWeightDoc(){
  return [
    { name:'weight_minimal', node_type:'tab', parameters:{}, children:[
      { name:'', node_type:'div', parameters:{}, children:[
        { name:'WeightRecord', node_type:'div', parameters:{}, children:[
          { name:'date', node_type:'timestamp', parameters:{ precision:{ String:'day' }, value:{ Timestamp: '2025.08.24' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'test' } }, children:[], is_hierarchy_transparent:false }
        ], is_hierarchy_transparent:false }
      ], is_hierarchy_transparent:true },
      { name:'Selected', node_type:'div', parameters:{}, children:[
        { name:'selected_date', node_type:'timestamp', parameters:{ precision:{ String:'day' }, value:{ Timestamp:'2025.08.24' } }, children:[], is_hierarchy_transparent:false },
        { name:'SelectedWeightRecord', node_type:'div', parameters:{ link:{ String:'/weight_minimal/History[key=$(../selected_date)]' } }, children:[], is_hierarchy_transparent:false },
        { name:'Prev', node_type:'button', parameters:{ label:{ String:'< Prev Day' } }, children:[], is_hierarchy_transparent:false },
        { name:'Next', node_type:'button', parameters:{ label:{ String:'> Next Day' } }, children:[], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false },
      { name:'History', node_type:'list', parameters:{ entry:{ Template:'WeightRecord' }, key:{ String:'date' }, keyPrecision:{ String:'day' } }, children:[
        { name:'WeightRecord__1', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.08.27' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Wednesday' } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
        { name:'WeightRecord__2', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.08.26' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Wednesday' } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
        { name:'WeightRecord__3', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.08.25' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Tuesday' } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
        { name:'WeightRecord__4', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.08.24' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Monday' } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false }
    ], is_hierarchy_transparent:false }
  ]
}

function getDateTextFromLinkContainer(linkContainer){
  // Prefer element whose data-path ends with 'date'
  const allWithPath = Array.from(linkContainer.querySelectorAll('[data-path]'))
  for (const el of allWithPath) {
    try {
      const p = JSON.parse(el.dataset.path||'[]')
      if (Array.isArray(p) && p[p.length-1] === 'date') {
        const valueEl = el.querySelector('.field-value, .text-content') || el
        return (valueEl.textContent||'').trim()
      }
    } catch(_){/* ignore */}
  }
  // Fallback to any value element
  const valueEl = linkContainer.querySelector('.field-value, .text-content') || linkContainer
  return (valueEl.textContent||'').trim()
}

function normDayString(s){
  const str = (s||'').toString().trim()
  const m = str.match(/(\d{4})[.\-](\d{2})[.\-](\d{2})/)
  if (!m) return ''
  return `${m[1]}.${m[2]}.${m[3]}`
}

describe('link proxy phantom preview shows selected_date', () => {
  beforeEach(() => setupDOM())

  it('displays the selected_date in the phantom preview and updates on change', async () => {
    const { invoke } = await import('@tauri-apps/api/core')

    let currentDoc = buildWeightDoc()

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

    const linkPath = ['weight_minimal','Selected','SelectedWeightRecord']
    const selectedDatePath = ['weight_minimal','Selected','selected_date']

    const assertDateShown = (expected) => {
      const linkContainer = findByPath(linkPath)
      expect(linkContainer).toBeTruthy()
      const shown = getDateTextFromLinkContainer(linkContainer)
      expect(normDayString(shown)).toBe(normDayString(expected))
    }

    // 1) Initially selected_date = 2025.08.24 exists -> shows 2025.08.24
    assertDateShown('2025.08.24')

    // 2) Change to a missing date -> phantom should show the selected date
    const selectedNode = app.renderer.findNodeByPath(app.currentDocument, selectedDatePath)
    selectedNode.parameters.value = { String: '2025.09.07' }
    app.renderer.renderDocument(app.currentDocument)
    assertDateShown('2025.09.07')

    // 3) Change to an existing date -> shows the existing record's date
    selectedNode.parameters.value = { String: '2025.08.26' }
    app.renderer.renderDocument(app.currentDocument)
    assertDateShown('2025.08.26')

    // 4) Change to another missing date -> phantom updates accordingly
    selectedNode.parameters.value = { String: '2025.09.08' }
    app.renderer.renderDocument(app.currentDocument)
    assertDateShown('2025.09.08')
  })
})
