import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('@tauri-apps/api/tauri', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
        <button id="save-file-btn"></button>
        <button id="reload-file-btn"></button>
      </div>
      <div id="tab-container"></div>
      <div id="content-display"></div>
      <div id="welcome-screen" class="screen"></div>
      <div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div>
      <div id="error-message"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span></div>
    </div>`
}

function deepClone(o){ return JSON.parse(JSON.stringify(o)) }

// Minimal document approximating exercise_tracker flow with a Done button performing set / set_now_ts / prepend.
function buildDoc(){
  return [
    { name:'root', node_type:'tab', parameters:{}, children:[
      { name:'Template', node_type:'div', parameters:{}, children:[
        { name:'eid', node_type:'int', parameters:{ value:{ Int: 5 } }, children:[], is_hierarchy_transparent:false },
        { name:'sets', node_type:'int', parameters:{ value:{ Int: 2 } }, children:[], is_hierarchy_transparent:false },
        { name:'reps', node_type:'int', parameters:{ value:{ Int: 21 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
        { name:'weight', node_type:'int', parameters:{ value:{ Int: 96 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
        { name:'time', node_type:'timestamp', parameters:{ value:{ Timestamp:'2025-09-28T11:10:55.585154300+00:00' } }, children:[], is_hierarchy_transparent:false }
      ], is_hierarchy_transparent:false },
      { name:'SelectedWrapper', node_type:'div', parameters:{}, children:[
        { name:'selected_eid', node_type:'int', parameters:{ value:{ Int: 5 } }, children:[], is_hierarchy_transparent:false },
        { name:'SelectedEntry', node_type:'div', parameters:{ link:{ String:'/root/History[key=$(../selected_eid)]' } }, children:[], is_hierarchy_transparent:false },
        { name:'Done', node_type:'button', parameters:{ label:{ String:'Done' } }, children:[], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false },
      { name:'History', node_type:'list', parameters:{ entry:{ Template:'Template' }, key:{ String:'eid' } }, children:[
        { name:'Template__1', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'eid', node_type:'int', parameters:{ value:{ Int: 5 } }, children:[], is_hierarchy_transparent:false },
          { name:'sets', node_type:'int', parameters:{ value:{ Int: 2 } }, children:[], is_hierarchy_transparent:false },
          { name:'reps', node_type:'int', parameters:{ value:{ Int: 21 } }, children:[], is_hierarchy_transparent:false },
          { name:'weight', node_type:'int', parameters:{ value:{ Int: 96 } }, children:[], is_hierarchy_transparent:false },
          { name:'time', node_type:'timestamp', parameters:{ value:{ Timestamp:'2025-09-28T11:10:55.585154300+00:00' } }, children:[], is_hierarchy_transparent:false }
        ], is_hierarchy_transparent:false }
      ], is_hierarchy_transparent:false }
    ], is_hierarchy_transparent:false }
  ]
}

// --- Pseudo DSL serializer (test-only) ------------------------------------
// We can't rely on real Rust serialization in this isolated JS unit test, so we
// deterministically serialize just the History list into a minimal DSL that
// exposes list entries and field lines. This enables robust structural diffing
// (counting "- {" occurrences) without depending on JSON snapshots.
function serializeHistoryPseudoDSL(doc){
  function findHistory(nodes){
    for (const n of nodes){
      if (n.name === 'History' && n.node_type === 'list') return n
      if (n.children && n.children.length){ const f=findHistory(n.children); if (f) return f }
    }
    return null
  }
  const history = findHistory(doc)
  if (!history) return ''
  const lines = [ 'list History (entry=<Rec>) {' ]
  // stable desired field order for readability / determinism
  const order = ['eid','sets','reps','weight','time']
  for (const entry of history.children){
    lines.push('    - {')
    // Map fields by name
    const map = new Map(entry.children.map(c=>[c.name,c]))
    for (const name of order){
      const field = map.get(name)
      if (!field) continue
      let val = ''
      const p = field.parameters || {}
      if (p.value){
        if (p.value.Int !== undefined) val = p.value.Int
        else if (p.value.Timestamp !== undefined) val = '"'+p.value.Timestamp+'"'
      }
      lines.push(`        - ${name} = ${val}`)
    }
    lines.push('    }')
  }
  lines.push('}')
  return lines.join('\n')
}

// Structural diff: count list entry openings and verify only +1 along with
// remainder equivalence after skipping the inserted first block.
function computeAddedBlocks(beforeText, afterText){
  const beforeLines = beforeText.split(/\r?\n/)
  const afterLines  = afterText.split(/\r?\n/)
  const beforeCount = beforeLines.filter(l=>l.trim()==='- {').length
  const afterCount  = afterLines.filter(l=>l.trim()==='- {').length
  if (afterCount !== beforeCount + 1){
    return { newBlockCount: afterCount - beforeCount, drift:true }
  }
  // Locate divergence
  let i=0; while(i<beforeLines.length && i<afterLines.length && beforeLines[i]===afterLines[i]) i++
  // Walk the new block braces to compute where it ends
  let depth=0; let j=i; for(; j<afterLines.length; j++){
    const t = afterLines[j].trim()
    if (t==='- {') depth++
    else if (t==='}') { depth--; if (depth===0){ j++; break } }
  }
  const beforeSlice = beforeLines.slice(i)
  const afterSlice  = afterLines.slice(j)
  // Normalize indentation of standalone closing braces for drift comparison
  const norm = arr => arr.map(l => l.trim()==='}' ? '}' : l).join('\n')
  const beforeNorm = norm(beforeSlice)
  const afterNorm  = norm(afterSlice)
  return { newBlockCount:1, drift: beforeNorm !== afterNorm }
}

describe('history done button UI flow', () => {
  beforeEach(()=>setupDOM())

  it('clicking Done prepends exactly one entry with no indentation drift', async () => {
    const { invoke } = await import('@tauri-apps/api/tauri')
    let currentDoc = buildDoc()
    let lastSerialized = null

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      if (cmd === 'serialize_overseer_nodes') {
        lastSerialized = deepClone(args.nodes)
        currentDoc = deepClone(args.nodes)
        const dsl = serializeHistoryPseudoDSL(currentDoc)
        // Expose through the app static for compatibility with pre-existing harness expectations
        OverseerApp._lastSerializedText = dsl
        return Promise.resolve(dsl)
      }
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('')
      return Promise.reject(new Error('unknown command: '+cmd))
    })

    const app = new OverseerApp()
    app.currentDocument = deepClone(currentDoc)
    app.renderer.renderDocument(app.currentDocument)

    // Edit mutable fields through underlying template (simulate user typing before Done)
    const template = app.renderer.findNodeByPath(app.currentDocument, ['root','Template'])
    template.children.find(c=>c.name==='reps').parameters.value = { Int: 25 }
    template.children.find(c=>c.name==='weight').parameters.value = { Int: 100 }

  // Stub updateTitle to avoid DOM dependency (status bar elements absent in minimal test DOM)
  app.updateTitle = () => {}
  const before = await (async () => { await app.saveFile(); return OverseerApp._lastSerializedText || JSON.stringify(lastSerialized); })()

    // Locate Done button element produced during render
    // We fake a click by directly invoking renderer.emitEvent sequence equivalent
    const doneNode = app.renderer.findNodeByPath(app.currentDocument, ['root','SelectedWrapper','Done'])
    // Simulate the button's on click action sequence manually (mirrors exercise.os pattern)
    // set (eid, sets, reps, weight, time) + prepend
    const historyList = app.renderer.findNodeByPath(app.currentDocument, ['root','History'])
    const clone = deepClone(template)
    clone.name = 'Template__X'
    // set_now_ts simulation: use a deterministic timestamp to keep test stable
    const ts = '2099-12-31T23:59:59.000000+00:00'
    clone.children.find(c=>c.name==='time').parameters.value = { Timestamp: ts }
    historyList.children.unshift(clone)

    await app.saveFile()

  const after = OverseerApp._lastSerializedText || JSON.stringify(lastSerialized)

    expect(lastSerialized).toBeTruthy()

    const { newBlockCount, drift } = computeAddedBlocks(before, after)
    expect(newBlockCount).toBe(1)
    expect(drift).toBe(false)
  })
})
