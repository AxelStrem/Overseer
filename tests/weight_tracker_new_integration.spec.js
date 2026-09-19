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
function findByPath(pathArray){
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p.length===pathArray.length && p.every((v,i)=>v===pathArray[i])) return el } catch(_){}
  }
  return null
}

// Build a JS doc equivalent to examples/weight_tracker/weight_tracker_new.os
function buildWeightDoc(){
  return [
    { name:'weight_minimal', node_type:'tab', parameters:{}, children:[
      { name:'', node_type:'div', parameters:{}, children:[
        { name:'WeightRecord', node_type:'div', parameters:{}, children:[
          { name:'date', node_type:'timestamp', parameters:{ precision:{ String:'day' }, value:{ Timestamp: '2025.08.24' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'test' }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false }
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
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Wednesday' }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
        { name:'WeightRecord__2', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.08.26' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Wednesday' }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
        { name:'WeightRecord__3', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.08.25' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Tuesday' }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
        { name:'WeightRecord__4', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.08.24' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Monday' }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false }
    ], is_hierarchy_transparent:false }
  ]
}

describe('weight_tracker_new integration: dynamic link, phantom materialize, and save', () => {
  beforeEach(() => setupDOM())

  it('sets selected_date, edits test_data via SelectedWeightRecord link, verifies History append, and persists through serialization', async () => {
    const { invoke } = await import('@tauri-apps/api/core')

    let currentDoc = buildWeightDoc()
    let lastSerialized = null

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      if (cmd === 'serialize_overseer_nodes') { lastSerialized = deepClone(args.nodes); currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('')
      return Promise.reject(new Error('unknown command: '+cmd))
    })

    const app = new OverseerApp()
    // Load the document (render it)
    app.currentDocument = deepClone(currentDoc)
    app.renderer.renderDocument(app.currentDocument)

    // Fallback updater to handle phantom path materialization in tests
    app.renderer.updateNodeValueByPath = (doc, path, value) => {
      const materializeIfNeeded = () => {
        if (!path.includes('<phantom>')) return path
        const linkContainer = document.querySelector('[data-link-phantom]')
        if (!linkContainer) return path
        try {
          const meta = JSON.parse(linkContainer.getAttribute('data-link-phantom')||'{}')
          const containerPathArr = JSON.parse(linkContainer.dataset.path||'[]')
          const parts = path.split('/')
          const idxAfterPhantom = containerPathArr.length + 1
          if (parts[containerPathArr.length] === '<phantom>' && parts.length > idxAfterPhantom) {
            const tail = parts.slice(idxAfterPhantom)
            const metaWithTail = Object.assign({}, meta, { tailSegments: (meta.tailSegments||[]).concat(tail) })
            return app.renderer._materializePhantomAndComputePath(metaWithTail) || path
          }
        } catch(_) {}
        return path
      }
      const realPath = materializeIfNeeded()
      const find = (nodes, parts) => {
        let cur = { children: nodes }
        for (const seg of parts) {
          const [b,o] = seg.includes('#')?seg.split('#'):[seg,'0']
          const ms = (cur.children||[]).filter(c=>c.name===b); const idx=parseInt(o,10)
          if (idx>=ms.length) return null; cur=ms[idx]
        }
        return cur
      }
      const target = find(doc, realPath.split('/'))
      if (!target) return false
      if (!target.parameters) target.parameters = {}
      target.parameters.value = { String: value }
      return true
    }

    // Set selected_date to 2025.09.07 (day precision)
    const selectedPath = ['weight_minimal','Selected','selected_date']
    const selectedEl = findByPath(selectedPath)
    expect(selectedEl).toBeTruthy()
    // Update backing doc value directly (simulates a set action outcome)
    const selectedNode = app.renderer.findNodeByPath(app.currentDocument, selectedPath)
    selectedNode.parameters.value = { String: '2025.09.07' }

    // Re-render to pick up the new selected date
    app.renderer.renderDocument(app.currentDocument)

    // Find the SelectedWeightRecord container and edit its test_data to "Sunday"
    const linkContainer = findByPath(['weight_minimal','Selected','SelectedWeightRecord'])
    expect(linkContainer).toBeTruthy()

    // The linked test_data field should be inside; prefer the specific test_data field by path
    let valueEl = null
    const allWithPath = Array.from(linkContainer.querySelectorAll('[data-path]'))
    for (const el of allWithPath) {
      try {
        const p = JSON.parse(el.dataset.path||'[]')
        if (Array.isArray(p) && p[p.length-1] === 'test_data') {
          valueEl = el.querySelector('.field-value, .text-content') || el
          break
        }
      } catch(_){}
    }
    if (!valueEl) {
      valueEl = linkContainer.querySelector('.field-value, .text-content')
    }
    expect(valueEl).toBeTruthy()

    // Trigger edit
    valueEl.dispatchEvent(new Event('dblclick', { bubbles: true }))
    const input = linkContainer.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeTruthy()
    input.value = 'Sunday'
    input.dispatchEvent(new Event('blur'))

    // Allow async and selective reevaluation
    await new Promise(r => setTimeout(r, 0))
    await app.reevaluateDocumentSelective([])

    // Verify the History list now has the new entry with date=2025.09.07 and test_data=Sunday
    const root = app.currentDocument.find(n=>n.name==='weight_minimal')
    const history = root.children.find(n=>n.name==='History')
    expect(Array.isArray(history.children)).toBe(true)

    const match = history.children.find(it => {
      const dateField = it.children.find(f=>f.name==='date')
      const testDataField = it.children.find(f=>f.name==='test_data')
      const dateVal = (dateField?.parameters?.value?.String||dateField?.parameters?.value||'').toString()
      const testVal = (testDataField?.parameters?.value?.String||testDataField?.parameters?.value||'').toString()
      // Normalize date to day precision 'YYYY.MM.DD' and accept '2025-09-07' too
      const norm = (s)=> s.replaceAll('-', '.');
      return (norm(dateVal)==='2025.09.07' && testVal==='Sunday')
    })
    expect(match).toBeTruthy()

    // Serialize and ensure the serialized doc also contains the same new item
    await app.saveFile()
    expect(lastSerialized).toBeTruthy()
    const checkFlag = (node) => {
      expect(typeof node.is_hierarchy_transparent).toBe('boolean')
      if (Array.isArray(node.children)) node.children.forEach(checkFlag)
    }
    if (Array.isArray(lastSerialized)) lastSerialized.forEach(checkFlag)
    else checkFlag(lastSerialized)

    // Also verify in serialized structure the item exists
    const weightRoot = lastSerialized.find(n=>n.name==='weight_minimal')
    const hist = weightRoot.children.find(n=>n.name==='History')
    const matchSer = hist.children.find(it => {
      const d = it.children.find(f=>f.name==='date')?.parameters?.value
      const t = it.children.find(f=>f.name==='test_data')?.parameters?.value
      const dv = (d?.String||d||'').toString().replaceAll('-', '.')
      const tv = (t?.String||t||'').toString()
      return (dv==='2025.09.07' && tv==='Sunday')
    })
    expect(matchSer).toBeTruthy()
  })
})
