import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

function setupDOM(){
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
  </div>`
}

function buildDoc(){
  // Mirrors examples/basic/unnamed_divs.os but uses pipeline map/sum variant now in example file
  // int total = $(L.map(|x| x/C).sum())
  return [
    { name:'main', node_type:'tab', parameters:{}, is_hierarchy_transparent:true, children:[
      { name:'', node_type:'div', parameters:{ hidden:{ Boolean:true } }, is_hierarchy_transparent:true, children:[
        { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
          { name:'T', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
            { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
              { name:'A', node_type:'int', parameters:{ label:{ String:'X' }, value:{ Integer:1 } }, children:[], is_hierarchy_transparent:false }
            ]},
            { name:'B', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false },
            { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false },
          ]}
        ]}
      ]},
      { name:'total', node_type:'int', parameters:{ value:{ Formula:'L.map(|x| x/C).sum()' } }, children:[], is_hierarchy_transparent:false },
      { name:'L', node_type:'list', parameters:{ entry:{ Template:'T' } }, children:[
        { name:'T__1', node_type:'div', parameters:{ _from_template:true, _original_type:'T' }, is_hierarchy_transparent:false, children:[
          { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
            { name:'A', node_type:'int', parameters:{ label:{ String:'X' }, value:{ Integer:1 } }, children:[], is_hierarchy_transparent:false }
          ]},
          { name:'B', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false },
          { name:'C', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false },
        ]}
      ], is_hierarchy_transparent:false }
    ]}
  ]
}

// Helper to find element by path array
function findByPath(pathArr){
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all){
    try{ const p = JSON.parse(el.dataset.path||'[]'); if(Array.isArray(p)&&p.length===pathArr.length&&p.every((v,i)=>v===pathArr[i])) return el }catch(e){}
  }
  return null
}

async function editInt(app, path, newValue){
  const segs = path.split('/')
  let node = null
  let nodes = app.currentDocument
  // Walk allowing unnamed '' transparent nodes to be skipped implicitly
  for (let i=0;i<segs.length;i++){
    const targetSeg = segs[i]
    let found = nodes.find(n=>n.name===targetSeg)
    if(!found){
      // Try descending into unnamed transparent wrappers to find next segment
      for (const maybe of nodes.filter(n=>n.name==='')){
        const deep = maybe.children.find(c=>c.name===targetSeg)
        if (deep){ found = deep; break }
      }
    }
    node = found
    if(!node) break
    nodes = node.children||[]
  }
  expect(node).toBeTruthy()
  if (!node.parameters) node.parameters = {}
  node.parameters.value = { Integer: newValue }
  await app.reevaluateDocumentSelective([path], [{ path, oldValue: 'X', newValue: String(newValue) }])
}

describe('Unnamed divs: total aggregate updates after editing A or B', () => {
  beforeEach(() => setupDOM())

  it('Editing A updates C and total; editing B also updates total', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    let currentDoc = buildDoc()
    // Seed initial computed values for C (A*B=2) and total (=2) so renderer displays them immediately
    const listNode = currentDoc[0].children.find(n=>n.name==='L')
    const item = listNode.children[0]
    const cNode = item.children.find(n=>n.name==='C')
    if (cNode) cNode.parameters._computed_value = { Integer: 2 }
    const totalField = currentDoc[0].children.find(n=>n.name==='total')
    if (totalField) totalField.parameters._computed_value = { Integer: 2 }
    // Helper: deep clone
    const clone = o => JSON.parse(JSON.stringify(o))
    // Backend selective recompute stub: recompute C (A*B) and total (sum of C across items via map-sum) when A or B changes
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') {
        return Promise.resolve('DOC')
      }
      if (cmd === 'parse_overseer_content_selective') {
        // mimic backend returning fully updated tree (easier than patch path for this test)
        const list = currentDoc[0].children.find(n=>n.name==='L')
        let sum = 0
        if (list){
          for (const item of list.children){
            const aWrap = item.children.find(ch=>ch.name==='')
            const aNode = aWrap?.children.find(c=>c.name==='A')
            const bNode = item.children.find(c=>c.name==='B')
            const cNode = item.children.find(c=>c.name==='C')
            const aval = aNode?.parameters?.value?.Integer
            const bval = bNode?.parameters?.value?.Integer
            if (typeof aval==='number' && typeof bval==='number' && cNode){
              const prod = aval * bval
              // Reflect computed product the way real backend would: preserve raw value when it is a formula, write to _computed_value.
              // Our test list instance has a concrete integer "value" (no formula) for C, so update both value and _computed_value
              // to make DOM display logic unambiguous.
              cNode.parameters.value = { Integer: prod }
              cNode.parameters._computed_value = { Integer: prod }
              sum += prod
            }
          }
        }
        const totalField = currentDoc[0].children.find(n=>n.name==='total')
        if (totalField) {
          // Always preserve the formula in value and surface numeric result via _computed_value only
          totalField.parameters.value = { Formula: 'L.map(|x| x/C).sum()' }
          totalField.parameters._computed_value = { Integer: sum }
        }
        return Promise.resolve(clone(currentDoc))
      }
      if (cmd === 'parse_overseer_content') { return Promise.resolve(clone(currentDoc)) }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      return Promise.reject(new Error('Unhandled invoke '+cmd))
    })

  const app = new OverseerApp()
  app.currentDocument = currentDoc
  // Render using same pattern as other UI tests
  app.renderer.renderDocument(app.currentDocument)
    // Initial total is 2 (A=1,B=2,C=2)
  let totalElContainer = findByPath(['main','total']) || findByPath(['main','total','value'])
    expect(totalElContainer).toBeTruthy()
    let totalValueEl = totalElContainer.querySelector('.field-value') || totalElContainer
    expect((totalValueEl.textContent||'').trim()).toBe('2')

    // Edit A to 5 -> C=10 total=10
  await editInt(app, 'main/L/T__1/A', 5)
  app.renderer.renderDocument(app.currentDocument)
  // Sync backing doc reference for stub so subsequent selective recomputes use latest mutated tree
  currentDoc = app.currentDocument
  let cEl = findByPath(['main','L','T__1','C']) || findByPath(['main','L','T__1','C','value'])
    expect(cEl).toBeTruthy()
    totalElContainer = findByPath(['main','total'])
    totalValueEl = totalElContainer.querySelector('.field-value') || totalElContainer
    expect((totalValueEl.textContent||'').trim()).toBe('10')

    // Edit B to 3 -> C=15 total=15
  await editInt(app, 'main/L/T__1/B', 3)
  // Sync again after second edit (not strictly needed after fix but keeps pattern consistent)
  currentDoc = app.currentDocument
  // Debug instrumentation to understand why total not updating to 15
  if (true) {
    const listNode2 = app.currentDocument[0].children.find(n=>n.name==='L')
    const item2 = listNode2.children[0]
    const aWrap2 = item2.children.find(ch=>ch.name==='')
    const a2 = aWrap2?.children.find(c=>c.name==='A')?.parameters?.value
    const b2 = item2.children.find(c=>c.name==='B')?.parameters?.value
    const c2 = item2.children.find(c=>c.name==='C')?.parameters
    const total2 = app.currentDocument[0].children.find(n=>n.name==='total')?.parameters
    // eslint-disable-next-line no-console
    console.log('[AGG DEBUG] After B edit: A=', a2, 'B=', b2, 'C params=', c2, 'total params=', total2)
  }
  app.renderer.renderDocument(app.currentDocument)
  cEl = findByPath(['main','L','T__1','C']) || findByPath(['main','L','T__1','C','value'])
    expect(cEl).toBeTruthy()
    totalElContainer = findByPath(['main','total'])
    totalValueEl = totalElContainer.querySelector('.field-value') || totalElContainer
    expect((totalValueEl.textContent||'').trim()).toBe('15')
  })
})
