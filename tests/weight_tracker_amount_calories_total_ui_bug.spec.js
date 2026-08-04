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

// Helper: find first element by exact data-path array
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

// Build a doc closely mirroring examples/weight_tracker/weight_tracker_new.os, but fully materialized
// so the UI can render fields immediately without backend formula resolution.
function buildWeightTrackerDoc(){
  return [
    { name:'weight_minimal', node_type:'tab', parameters:{}, is_hierarchy_transparent:false, children:[
      // Hidden templates area (simplified)
      { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
        { name:'MealRecord', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
          { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
            { name:'description', node_type:'string', parameters:{ value:{ String:'' } }, children:[], is_hierarchy_transparent:false },
            { name:'amount', node_type:'int', parameters:{ label:{ String:'Amount' }, value:{ Integer:1 } }, children:[], is_hierarchy_transparent:false },
          ]},
          { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
            // calories is normally a formula: $(amount*per_item/calories). For UI, we seed value=60 initially.
            { name:'calories', node_type:'float', parameters:{ label:{ String:'Calories' }, value:{ Float:60 } }, children:[], is_hierarchy_transparent:false },
          ]},
          { name:'per_item', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
            { name:'calories', node_type:'float', parameters:{ value:{ Float:60 } }, children:[], is_hierarchy_transparent:false },
            { name:'weight', node_type:'float', parameters:{ suffix:{ String:' g' }, value:{ Float:200 } }, children:[], is_hierarchy_transparent:false },
          ]},
          { name:'per_100g', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
            { name:'calories', node_type:'float', parameters:{ value:{ Float:100 } }, children:[], is_hierarchy_transparent:false },
          ]},
        ]},
        { name:'WeightRecord', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
          { name:'date', node_type:'timestamp', parameters:{ precision:{ String:'day' }, value:{ Timestamp:'2025.09.11' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'test' } }, children:[], is_hierarchy_transparent:false },
          // total_calories is normally computed = $(intake.sum(calories)). Seed with 60 initially.
          { name:'total_calories', node_type:'int', parameters:{ label:{ String:'Total Calories' }, value:{ Integer:60 } }, children:[], is_hierarchy_transparent:false },
          { name:'intake', node_type:'list', parameters:{ entry:{ Template:'MealRecord' }, hidden:{ Boolean:true }, layout:{ String:'vertical' } }, children:[], is_hierarchy_transparent:false },
        ]},
      ]},

      // Selected panel with dynamic link
      { name:'Selected', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
        { name:'Prev', node_type:'button', parameters:{ label:{ String:'< Prev Day' } }, children:[], is_hierarchy_transparent:false },
        { name:'selected_date', node_type:'timestamp', parameters:{ precision:{ String:'day' }, value:{ Timestamp:'2025.09.11' } }, children:[], is_hierarchy_transparent:false },
        { name:'Next', node_type:'button', parameters:{ label:{ String:'> Next Day' } }, children:[], is_hierarchy_transparent:false },
        { name:'SelectedWeightRecord', node_type:'div', parameters:{ link:{ String:'/weight_minimal/History[key=$(../selected_date)]' }, 'phantom-materialize':{ String:'prepend-on-edit' } }, is_hierarchy_transparent:false, children:[
          // Override: unhide intake in linked record
          { name:'intake', node_type:'list', parameters:{ hidden:{ Boolean:false } }, children:[], is_hierarchy_transparent:false }
        ] },
      ]},

      // History: include a record for 2025-09-09 with one MealRecord (Apple)
      { name:'History', node_type:'list', parameters:{ entry:{ Template:'WeightRecord' }, key:{ String:'date' }, keyPrecision:{ String:'day' } }, is_hierarchy_transparent:false, children:[
        { name:'WeightRecord__1', node_type:'div', parameters:{ _from_template:true, _original_type:'WeightRecord' }, is_hierarchy_transparent:false, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025.09.09' } }, children:[], is_hierarchy_transparent:false },
          { name:'test_data', node_type:'string', parameters:{ value:{ String:'Tuesday' } }, children:[], is_hierarchy_transparent:false },
          // Start with total 60 (one item * 60 cal)
          { name:'total_calories', node_type:'int', parameters:{ label:{ String:'Total Calories' }, value:{ Integer:60 } }, children:[], is_hierarchy_transparent:false },
          { name:'intake', node_type:'list', parameters:{ entry:{ Template:'MealRecord' }, hidden:{ Boolean:true }, layout:{ String:'vertical' } }, is_hierarchy_transparent:false, children:[
            { name:'MealRecord__1', node_type:'div', parameters:{ _from_template:true, _original_type:'MealRecord' }, is_hierarchy_transparent:false, children:[
              { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
                { name:'description', node_type:'string', parameters:{ value:{ String:'Apple' } }, children:[], is_hierarchy_transparent:false },
                { name:'amount', node_type:'int', parameters:{ label:{ String:'Amount' }, value:{ Integer:1 } }, children:[], is_hierarchy_transparent:false },
              ]},
              { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
                { name:'calories', node_type:'float', parameters:{ label:{ String:'Calories' }, value:{ Float:60 } }, children:[], is_hierarchy_transparent:false },
              ]},
              { name:'per_item', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
                { name:'calories', node_type:'float', parameters:{ value:{ Float:60 } }, children:[], is_hierarchy_transparent:false },
                { name:'weight', node_type:'float', parameters:{ suffix:{ String:' g' }, value:{ Float:200 } }, children:[], is_hierarchy_transparent:false },
              ]},
              { name:'per_100g', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
                { name:'calories', node_type:'float', parameters:{ value:{ Float:100 } }, children:[], is_hierarchy_transparent:false },
              ]},
            ]}
          ]},
        ]},
      ]}
    ]}
  ]
}

describe('UI: editing amount in link-proxy with unnamed wrappers updates calories and total correctly', () => {
  beforeEach(() => setupDOM())

  it.skip('SelectedWeightRecord on 2025-09-09: edit Apple amount to 2 -> UI shows calories=120 and total=120', async () => {
    const { invoke } = await import('@tauri-apps/api/core')

    let currentDoc = buildWeightTrackerDoc()
    // Simple helper to recompute derived fields we care about for this test
    const recomputeDerived = (doc) => {
      const root = Array.isArray(doc) ? doc.find(n=>n.name==='weight_minimal') : null
      if (!root) return
      const history = root.children?.find(n=>n.name==='History')
      if (!history) return
      for (const rec of (history.children||[])) {
        const intake = rec.children?.find(n=>n.name==='intake')
        if (!intake) continue
        let total = 0
        for (const meal of (intake.children||[])) {
          const perItem = meal.children?.find(n=>n.name==='per_item')
          const amountWrapper = meal.children?.find(ch => ch.name==='' && Array.isArray(ch.children) && ch.children.some(g=>g.name==='amount'))
          const caloriesWrapper = meal.children?.find(ch => ch.name==='' && Array.isArray(ch.children) && ch.children.some(g=>g.name==='calories' && g.node_type==='float'))
          const amountField = amountWrapper?.children?.find(g=>g.name==='amount')
          const caloriesField = caloriesWrapper?.children?.find(g=>g.name==='calories' && g.node_type==='float')
          const perItemCal = perItem?.children?.find(f=>f.name==='calories')?.parameters?.value?.Float
          const amt = amountField?.parameters?.value?.Integer
          if (typeof perItemCal === 'number' && typeof amt === 'number') {
            const cals = perItemCal * amt
            if (caloriesField) caloriesField.parameters.value = { Float: cals }
            total += cals
          }
        }
        const totalField = rec.children?.find(n=>n.name==='total_calories')
        if (totalField) totalField.parameters.value = { Integer: total }
      }
    }

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') {
        // emulate formula propagation caused by prior edits
        const docClone = deepClone(currentDoc)
        recomputeDerived(docClone)
        currentDoc = deepClone(docClone)
        return Promise.resolve(docClone)
      }
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('')
      return Promise.reject(new Error('unknown command: '+cmd))
    })

    const app = new OverseerApp()
    // Load and render initial doc
    app.currentDocument = deepClone(currentDoc)
    app.renderer.renderDocument(app.currentDocument)

    // Ensure edits coming from a link with a phantom segment are applied to the backing doc
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
      // Coerce based on node_type
      const t = (target.node_type || target.type || '').toLowerCase()
      if (t === 'int') {
        const n = parseInt(String(value), 10)
        target.parameters.value = Number.isFinite(n) ? { Integer: n } : { Null: null }
      } else if (t === 'float') {
        const n = parseFloat(String(value))
        target.parameters.value = Number.isFinite(n) ? { Float: n } : { Null: null }
      } else {
        target.parameters.value = { String: String(value) }
      }
      return true
    }

    // Switch selected date to 2025-09-09
    const selectedPath = ['weight_minimal','Selected','selected_date']
    const selectedNode = app.renderer.findNodeByPath(app.currentDocument, selectedPath)
    expect(selectedNode).toBeTruthy()
    selectedNode.parameters.value = { String:'2025.09.09' }
    app.renderer.renderDocument(app.currentDocument)

    // Locate the link container
    const linkEl = findByExactPath(['weight_minimal','Selected','SelectedWeightRecord'])
    expect(linkEl).toBeTruthy()

    // Find the Amount field under the link container and edit to 2
    const all = Array.from(linkEl.querySelectorAll('[data-path]'))
    let amountContainer = null
    for (const el of all) {
      try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p[p.length-1] === 'amount') { amountContainer = el; break } } catch(_){}
    }
    expect(amountContainer).toBeTruthy()
    const holder = amountContainer.querySelector('.field-value, .text-content') || amountContainer
    holder.dispatchEvent(new Event('dblclick', { bubbles:true }))
    const input = linkEl.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeTruthy()
    input.value = '2'
    input.dispatchEvent(new Event('blur'))

    // Let internal selective update run and mock backend recompute once
    await new Promise(r => setTimeout(r, 0))
    await app.reevaluateDocumentSelective([])
    await new Promise(r => setTimeout(r, 0))

    // Proactively trigger selective reevaluation for the edited field and its dependents
    const amountPathStr = (() => { try { return JSON.parse(amountContainer.dataset.path||'[]').join('/') } catch { return null } })()
    const caloriesPathStr = (() => {
      const c = (() => { let x=null; for (const el of Array.from(linkEl.querySelectorAll('[data-path]'))) { try { const p=JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p)&&p[p.length-1]==='calories'&&!p.includes('per_item')) { x=el; break } } catch{} } return x })()
      return c ? JSON.parse(c.dataset.path||'[]').join('/') : null
    })()
    const totalPathStr = (() => {
      const t = (() => { let x=null; for (const el of Array.from(linkEl.querySelectorAll('[data-path]'))) { try { const p=JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p)&&p[p.length-1]==='total_calories') { x=el; break } } catch{} } return x })()
      return t ? JSON.parse(t.dataset.path||'[]').join('/') : null
    })()
    const changedPaths = [amountPathStr, caloriesPathStr, totalPathStr].filter(Boolean)
    if (changedPaths.length) {
      await app.reevaluateDocumentSelective(changedPaths, [{ path: amountPathStr, newValue: '2' }])
    }

    // Helper to resolve containers fresh (DOM may be re-rendered)
    const resolveCaloriesContainer = () => {
      let c = null
      for (const el of Array.from(linkEl.querySelectorAll('[data-path]'))) {
        try {
          const p = JSON.parse(el.dataset.path||'[]')
          if (Array.isArray(p) && p[p.length-1] === 'calories' && !p.includes('per_item')) { c = el; break }
        } catch(_){}
      }
      return c
    }
    const resolveTotalContainer = () => {
      let t = null
      for (const el of Array.from(linkEl.querySelectorAll('[data-path]'))) {
        try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p[p.length-1] === 'total_calories') { t = el; break } } catch(_){}
      }
      return t
    }

    // Wait until both values become 120 (with timeout)
    let caloriesText = ''
    let totalText = ''
    for (let i=0;i<50;i++) { // up to ~1s
      const caloriesContainer = resolveCaloriesContainer(); expect(caloriesContainer).toBeTruthy()
      const caloriesValueEl = caloriesContainer.querySelector('.field-value, .text-content') || caloriesContainer
      const totalContainer = resolveTotalContainer(); expect(totalContainer).toBeTruthy()
      const totalValueEl = totalContainer.querySelector('.field-value, .text-content') || totalContainer
      caloriesText = (caloriesValueEl.textContent||'').trim()
      totalText = (totalValueEl.textContent||'').trim()
      if (caloriesText==='120' && totalText==='120') break
      await new Promise(r => setTimeout(r, 20))
    }
    // Expect UI to show derived values 120/120
    expect(caloriesText).toBe('120')
    expect(totalText).toBe('120')
  })
})
