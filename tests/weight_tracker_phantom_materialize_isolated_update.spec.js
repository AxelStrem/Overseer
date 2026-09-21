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
import { answerEnsureEntry } from './helpers/ensure-entry.js'

function deepClone(o){ return JSON.parse(JSON.stringify(o)) }
function findByPath(pathArray){
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p.length===pathArray.length && p.every((v,i)=>v===pathArray[i])) return el } catch(_){}
  }
  return null
}

function buildDoc(){
  return [
    { name:'weight_minimal', node_type:'tab', parameters:{}, children:[
      // Template definition used by History(entry=<WeightRecord>)
      { name:'WeightRecord', node_type:'div', parameters:{}, children:[
        { name:'date', node_type:'timestamp', parameters:{ precision:{ String:'day' }, value:{ String:'2025-09-01' } } },
        { name:'weight', node_type:'float', parameters:{ value:{ Float: 90.0 } } }
      ] },
      { name:'Selected', node_type:'div', parameters:{}, children:[
        { name:'selected_date', node_type:'timestamp', parameters:{ precision:{ String:'day' }, value:{ String:'2025-09-10' } } },
        { name:'SelectedWeightRecord', node_type:'div', parameters:{ link:{ String:'/weight_minimal/History[key=$(../selected_date)]' }, 'phantom-materialize':{ String:'prepend-on-edit' } }, children:[] },
      ] },
      { name:'History', node_type:'list', parameters:{ entry:{ Template:'WeightRecord' }, key:{ String:'date' }, keyPrecision:{ String:'day' } }, children:[
        { name:'WeightRecord__1', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025-09-09' } } },
          { name:'weight', node_type:'float', parameters:{ value:{ Float: 92.3 } } }
        ] },
        { name:'WeightRecord__2', node_type:'div', parameters:{ _from_template:true }, children:[
          { name:'date', node_type:'timestamp', parameters:{ value:{ String:'2025-09-08' } } },
          { name:'weight', node_type:'float', parameters:{ value:{ Float: 93.7 } } }
        ] }
      ] }
    ] }
  ]
}

describe('phantom materialize updates only the new item', () => {
  beforeEach(() => setupDOM())

  it('editing weight via SelectedWeightRecord does not change last existing item', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    let currentDoc = buildDoc()

    invoke.mockImplementation((cmd, args) => {
      // Making a preview real is a backend instruction now; the page used to do it itself in
      // its own copy of the document. See `helpers/ensure-entry.js`.
      const madeReal = answerEnsureEntry(() => app, cmd, args)
      if (madeReal !== null) return madeReal
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      // Answered from the document as it now stands, not from a copy captured the last time
      // the page serialized one: a change that goes as an instruction serializes nothing, so
      // such a copy predates the write and answering with it would undo it.
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(app.currentDocument))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(app.currentDocument))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('')
      return Promise.reject(new Error('unknown command: '+cmd))
    })

    const app = new OverseerApp()
    // Opened from somewhere, which every document a view is edited through is:
    // making a preview real is a write, and a write needs a file to reach.
    app.currentFile = '/documents/test.os'
    app.currentDocument = deepClone(currentDoc)
    app.renderer.renderDocument(app.currentDocument)

    const container = findByPath(['weight_minimal','Selected','SelectedWeightRecord'])
    expect(container).toBeTruthy()

    // Find or create the weight field element inside the phantom preview
    let weightEl = null
    const allWithPath = Array.from(container.querySelectorAll('[data-path]'))
    for (const el of allWithPath) {
      try {
        const p = JSON.parse(el.dataset.path||'[]')
        if (Array.isArray(p) && p[p.length-1] === 'weight') {
          weightEl = el.querySelector('.field-value, .text-content') || el
          break
        }
      } catch(_){}
    }
    if (!weightEl) {
      // Fall back to the first textual holder
      weightEl = container.querySelector('.field-value, .text-content')
    }
    expect(weightEl).toBeTruthy()

  // Explicitly materialize the phantom item first (prepend policy), then update weight via path API
  const meta = JSON.parse(container.getAttribute('data-link-phantom')||'{}')
  expect(meta && Array.isArray(meta.listPath)).toBeTruthy()
  const realItemPath = await app.renderer._materializePhantomAndComputePath(Object.assign({}, meta), { position: 'prepend' })
  expect(typeof realItemPath).toBe('string')
  const weightPath = `${realItemPath}/weight`
  const ok = app.renderer.updateNodeValueByPath(app.currentDocument, weightPath, '91.0')
  expect(ok).toBe(true)
  await app.reevaluateDocumentSelective([weightPath], [{ path: weightPath, oldValue: '', newValue: '91.0' }])

    // Validate: a new item for 2025-09-10 exists with weight 91.0
    const root = app.currentDocument.find(n=>n.name==='weight_minimal')
    const history = root.children.find(n=>n.name==='History')
    const itemNew = history.children.find(it => it.children.find(f=>f.name==='date')?.parameters?.value?.String === '2025-09-10')
    expect(itemNew).toBeTruthy()
  const wNew = itemNew.children.find(f=>f.name==='weight')?.parameters?.value
  const vNew = (wNew?.Float ?? parseFloat(wNew?.String ?? 'NaN'))
  expect(Number.isNaN(vNew)).toBe(false)
  expect(Number(vNew)).toBe(91.0)

    // And the last existing item (2025-09-08) retains its original value 93.7
    const itemLast = history.children.find(it => it.children.find(f=>f.name==='date')?.parameters?.value?.String === '2025-09-08')
    expect(itemLast).toBeTruthy()
  const wLast = itemLast.children.find(f=>f.name==='weight')?.parameters?.value
  const vLast = (wLast?.Float ?? parseFloat(wLast?.String ?? 'NaN'))
  expect(Number.isNaN(vLast)).toBe(false)
  expect(Number(vLast)).toBe(93.7)
  })
})
