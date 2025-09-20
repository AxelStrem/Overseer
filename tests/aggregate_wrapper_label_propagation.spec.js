import { describe, it, expect, vi, beforeEach } from 'vitest'
vi.mock('@tauri-apps/api/tauri', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

function setupDOM(){
  // Provide minimal screens + required elements used by app initialization
  document.body.innerHTML = `
    <div id="welcome-screen" class="screen"></div>
    <div id="editor-screen" class="screen"></div>
    <div id="error-screen" class="screen"><span id="error-message"></span></div>
    <div id="status-bar">
      <span id="status-message"></span>
      <span id="status-info"></span>
    </div>
    <span id="file-path"></span>
    <button id="save-file-btn"></button>
  `
}

function buildDoc() {
  // main -> hidden div -> T template with unnamed wrapper around A (with label) plus B and C
  return [
    { name:'main', node_type:'tab', is_hierarchy_transparent:true, parameters:{}, children:[
      { name:'L', node_type:'list', parameters:{ entry:{ Template:'T' } }, children:[
        { name:'T__1', node_type:'div', parameters:{ _from_template:true, _original_type:'T' }, is_hierarchy_transparent:false, children:[
          { name:'', node_type:'div', is_hierarchy_transparent:true, parameters:{}, children:[
            { name:'A', node_type:'int', parameters:{ value:{ Integer:1 }, label:{ String:'A'} }, children:[], is_hierarchy_transparent:false }
          ]},
          { name:'B', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false },
          { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B'} }, children:[], is_hierarchy_transparent:false }
        ]}
      ], is_hierarchy_transparent:false },
      { name:'T', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
        { name:'', node_type:'div', is_hierarchy_transparent:true, parameters:{}, children:[
          { name:'A', node_type:'int', parameters:{ value:{ Integer:1 }, label:{ String:'A'} }, children:[], is_hierarchy_transparent:false }
        ]},
        { name:'B', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false },
        { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B'} }, children:[], is_hierarchy_transparent:false }
      ]},
      { name:'total', node_type:'int', parameters:{ value:{ Formula:'L.map(|x| x/C).sum()'} }, children:[], is_hierarchy_transparent:false }
    ]}
  ]
}

describe('Wrapper+label propagation', () => {
  beforeEach(() => setupDOM())
  it('Editing A under unnamed wrapper with label propagates to C and total', async () => {
    const { invoke } = await import('@tauri-apps/api/tauri')
    let currentDoc = buildDoc()
    // crude mock backend: just echo currentDoc and (pretend) recompute C and total deterministically
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      if (cmd === 'parse_overseer_content_selective' || cmd === 'parse_overseer_content') {
        // Manually recompute C = A*B, total = sum of each C (single item)
        const clone = JSON.parse(JSON.stringify(currentDoc))
        const list = clone[0].children.find(n=>n.name==='L')
        const item = list.children[0]
        const wrap = item.children.find(c=>c.name==='')
        const A = wrap.children.find(c=>c.name==='A')
        const B = item.children.find(c=>c.name==='B')
        const C = item.children.find(c=>c.name==='C')
        const aval = A.parameters.value.Integer
        const bval = B.parameters.value.Integer
        C.parameters._computed_value = { Integer: aval * bval }
        // total
        const total = clone[0].children.find(n=>n.name==='total')
        total.parameters._computed_value = { Integer: aval * bval }
        return Promise.resolve(clone)
      }
      return Promise.reject(new Error('Unhandled cmd '+cmd))
    })
  const app = new OverseerApp()
    app.currentDocument = currentDoc
    // Edit A -> 5
    const item = app.currentDocument[0].children.find(n=>n.name==='L').children[0]
    const wrap = item.children.find(c=>c.name==='')
    const A = wrap.children.find(c=>c.name==='A')
    A.parameters.value = { Integer:5 }
    await app.reevaluateDocumentSelective(['main/L/T__1/A'], [{ path:'main/L/T__1/A', oldValue:1, newValue:5 }])
    const newDoc = app.currentDocument
    const newC = newDoc[0].children.find(n=>n.name==='L').children[0].children.find(c=>c.name==='C')
    const total = newDoc[0].children.find(n=>n.name==='total')
    const cVal = (newC.parameters._computed_value||newC.parameters.value).Integer
    const tVal = (total.parameters._computed_value||total.parameters.value).Integer
    expect(cVal).toBe(10) // 5 * 2
    expect(tVal).toBe(10)
  })
})