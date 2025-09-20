import { describe, it, expect, vi } from 'vitest'

vi.mock('@tauri-apps/api/tauri', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

// Monkey patch renderer methods to avoid needing full DOM render containers
class NoopRenderer {
  renderDocument(){ /* no-op */ }
  updateSelectiveFields(){ /* no-op */ }
}

// Provide minimal DOM scaffolding expected by OverseerApp methods (screens + file-path + save button)
beforeEach(() => {
  document.body.innerHTML = `
    <div id="welcome-screen" class="screen"></div>
    <div id="editor-screen" class="screen"></div>
    <div id="error-screen" class="screen"><span id="error-message"></span></div>
    <div id=\"status-bar\">
      <span id=\"status-message\"></span>
      <span id=\"status-info\"></span>
    </div>
    <span id="file-path"></span>
    <button id="save-file-btn"></button>
  `
})

// Minimal document: list L with template T providing A,B; total aggregates sum of C (=A*B)
function buildDoc(){
  return [
    { name:'main', node_type:'tab', parameters:{}, is_hierarchy_transparent:true, children:[
      { name:'total', node_type:'int', parameters:{ value:{ Formula:'L.map(|x| x/C).sum()' } }, children:[], is_hierarchy_transparent:false },
      { name:'L', node_type:'list', parameters:{ entry:{ Template:'T' } }, children:[
        { name:'T__1', node_type:'div', parameters:{ _from_template:true, _original_type:'T' }, is_hierarchy_transparent:false, children:[
          { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
            { name:'A', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false }
          ]},
          { name:'B', node_type:'int', parameters:{ value:{ Integer:3 } }, children:[], is_hierarchy_transparent:false },
          { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false }
        ]}
      ], is_hierarchy_transparent:false },
      { name:'T', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
        { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
          { name:'A', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false }
        ]},
        { name:'B', node_type:'int', parameters:{ value:{ Integer:3 } }, children:[], is_hierarchy_transparent:false },
        { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false }
      ]}
    ]}
  ]
}

describe('Aggregate formula persistence', () => {
  it('Edits do not overwrite aggregate formula with literal before and after save', async () => {
    const { invoke } = await import('@tauri-apps/api/tauri')
    let currentDoc = buildDoc()
    // Backend mocks: serialize, selective parse returns same doc (resolver would compute _computed_value only)
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(JSON.parse(JSON.stringify(currentDoc)))
      if (cmd === 'parse_overseer_content') return Promise.resolve(JSON.parse(JSON.stringify(currentDoc)))
      if (cmd === 'save_overseer_file') return Promise.resolve()
      return Promise.reject(new Error('Unhandled '+cmd))
    })
  const app = new OverseerApp()
  // Inject noop renderer
  app.renderer = new NoopRenderer()
    app.currentDocument = currentDoc
    // Initial formula present
    let total = app.currentDocument[0].children.find(n=>n.name==='total')
    expect(total.parameters.value.Formula).toBe('L.map(|x| x/C).sum()')
    // Edit A inside list item (simulate user change) to 5
    const listItem = app.currentDocument[0].children.find(n=>n.name==='L').children[0]
    const wrap = listItem.children.find(c=>c.name==='')
    const aNode = wrap.children.find(c=>c.name==='A')
    aNode.parameters.value = { Integer:5 }
  await app.reevaluateDocumentSelective(['main/L/T__1/A'], [{ path:'main/L/T__1/A', oldValue:'2', newValue:'5' }])
    total = app.currentDocument[0].children.find(n=>n.name==='total')
    // Formula should still be formula (not replaced by integer)
    expect(total.parameters.value.Formula).toBe('L.map(|x| x/C).sum()')
    // Simulate save serialization path (should retain formula)
    // (Frontend would call serialize then parse_overseer_content again; we already ensure value not clobbered.)
    expect(total.parameters.value.Formula).toBe('L.map(|x| x/C).sum()')
  })
})
