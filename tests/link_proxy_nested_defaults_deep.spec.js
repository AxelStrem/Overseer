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

// Pre-resolved deep nested document to validate front-end expectations
function buildDoc(){
  return [
    { name:'Root', node_type:'tab', parameters:{}, children:[
      { name:'T', node_type:'div', parameters:{ hidden:{ Boolean: true } }, children:[
        { name:'outer', node_type:'div', parameters:{}, children:[
          { name:'inner', node_type:'div', parameters:{}, children:[
            { name:'x', node_type:'float', parameters:{ value:{ Float: 1.0 } }, children:[], is_hierarchy_transparent:false },
            { name:'y', node_type:'float', parameters:{ value:{ Float: 2.0 } }, children:[], is_hierarchy_transparent:false }
          ], is_hierarchy_transparent:false },
          { name:'z', node_type:'float', parameters:{ value:{ Float: 3.0 } }, children:[], is_hierarchy_transparent:false }
        ], is_hierarchy_transparent:false }
      ], is_hierarchy_transparent:false },
      { name:'L', node_type:'list', parameters:{ entry:{ Template:'T' } }, children:[
        { name:'T__1', node_type:'T', parameters:{ _from_template:{ Boolean:true } }, children:[
          { name:'outer', node_type:'div', parameters:{}, children:[
            { name:'inner', node_type:'div', parameters:{}, children:[
              { name:'x', node_type:'float', parameters:{ value:{ Float: 10.0 } }, children:[], is_hierarchy_transparent:false },
              { name:'y', node_type:'float', parameters:{ value:{ Float: 2.0 } }, children:[], is_hierarchy_transparent:false }
            ], is_hierarchy_transparent:false },
            { name:'z', node_type:'float', parameters:{ value:{ Float: 3.0 } }, children:[], is_hierarchy_transparent:false }
          ], is_hierarchy_transparent:false }
        ], is_hierarchy_transparent:false }
      ], is_hierarchy_transparent:false }
    ], is_hierarchy_transparent:false }
  ]
}

describe('deep nested container defaults inside list item templates', () => {
  beforeEach(() => setupDOM())

  it('keeps defaults for inner sibling and outer cousin when overriding a deep child', async () => {
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

    const root = app.currentDocument.find(n=>n.name==='Root')
    const list = root.children.find(n=>n.name==='L')
    expect(list.children.length).toBe(1)
    const item = list.children[0]
    expect(item.node_type).toBe('T')

    const outer = item.children.find(n=>n.name==='outer')
    const inner = outer.children.find(n=>n.name==='inner')
    const x = inner.children.find(n=>n.name==='x')
    const y = inner.children.find(n=>n.name==='y')
    const z = outer.children.find(n=>n.name==='z')

    const xv = x.parameters?.value?.Float ?? x.parameters?.value
    const yv = y.parameters?.value?.Float ?? y.parameters?.value
    const zv = z.parameters?.value?.Float ?? z.parameters?.value
    expect(xv).toBe(10.0)
    expect(yv).toBe(2.0)
    expect(zv).toBe(3.0)
  })
})
