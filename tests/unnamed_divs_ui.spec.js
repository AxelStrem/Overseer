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
    try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p.length===pathArray.length && p.every((v,i)=>v===pathArray[i])) return el } catch(_){} }
  return null
}

// Helper: compute C = A*B for the first list entry (T__1), tolerant of unnamed transparent wrappers around A
function computeCProductForFirstListItem(doc){
  try {
    const roots = Array.isArray(doc) ? doc : [doc]
    const root = roots.find(n => n && n.name === 'Root') || roots[0]
    if (!root) return
    const L = (root.children || []).find(n => n && n.name === 'L')
    if (!L) return
    const item = (L.children || []).find(n => n && /^(T|T__1)(?:__\d+)?$/.test(n.name))
    if (!item) return
    const findTransparentAware = (node, want) => {
      const queue = [...(node.children||[])]
      while (queue.length){
        const ch = queue.shift()
        if (!ch) continue
        if (ch.name === want) return ch
        if (ch.is_hierarchy_transparent) {
          if (Array.isArray(ch.children)) queue.push(...ch.children)
        }
      }
      return null
    }
    const A = findTransparentAware(item, 'A')
    const B = (item.children || []).find(n => n && n.name === 'B')
    const C = (item.children || []).find(n => n && n.name === 'C')
    const a = A && A.parameters && A.parameters.value && (A.parameters.value.Integer ?? parseInt(A.parameters.value.String||A.parameters.value.Float||A.parameters.value,10))
    const b = B && B.parameters && B.parameters.value && (B.parameters.value.Integer ?? parseInt(B.parameters.value.String||B.parameters.value.Float||B.parameters.value,10))
    if (C && Number.isFinite(a) && Number.isFinite(b)) {
      if (!C.parameters) C.parameters = {}
      C.parameters._computed_value = { Integer: a * b }
    }
  } catch(_) { /* ignore compute errors in tests */ }
}

function buildDoc_withUnnamedWrapperAndLabel(){
  return [
    { name:'Root', node_type:'div', parameters:{}, children:[
      { name:'T', node_type:'div', parameters:{ hidden:{ Boolean:true } }, children:[
        { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
          { name:'A', node_type:'int', parameters:{ label:{ String:'X' }, value:{ Integer:1 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
        ]},
        { name:'B', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false },
        { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false },
      { name:'L', node_type:'list', parameters:{ entry:{ Template:'T' } }, children:[
        { name:'T__1', node_type:'T', parameters:{ _from_template:true }, children:[
          { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
            { name:'A', node_type:'int', parameters:{ label:{ String:'X' }, value:{ Integer:1 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
          ]},
          { name:'B', node_type:'int', parameters:{ value:{ Integer:2 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
          { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false }
    ], is_hierarchy_transparent:false }
  ]
}

function buildDoc_withoutUnnamedWrapper(){
  return [
    { name:'Root', node_type:'div', parameters:{}, children:[
      { name:'T', node_type:'div', parameters:{ hidden:{ Boolean:true } }, children:[
        { name:'A', node_type:'int', parameters:{ label:{ String:'X' }, value:{ Integer:1 } }, children:[], is_hierarchy_transparent:false },
        { name:'B', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false },
        { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false },
      { name:'L', node_type:'list', parameters:{ entry:{ Template:'T' } }, children:[
        { name:'T__1', node_type:'T', parameters:{ _from_template:true }, children:[
          { name:'A', node_type:'int', parameters:{ label:{ String:'X' }, value:{ Integer:1 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
          { name:'B', node_type:'int', parameters:{ value:{ Integer:2 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
          { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false }
    ], is_hierarchy_transparent:false }
  ]
}

function buildDoc_withoutLabel(){
  return [
    { name:'Root', node_type:'div', parameters:{}, children:[
      { name:'T', node_type:'div', parameters:{ hidden:{ Boolean:true } }, children:[
        { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
          { name:'A', node_type:'int', parameters:{ value:{ Integer:1 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
        ]},
        { name:'B', node_type:'int', parameters:{ value:{ Integer:2 } }, children:[], is_hierarchy_transparent:false },
        { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false },
      { name:'L', node_type:'list', parameters:{ entry:{ Template:'T' } }, children:[
        { name:'T__1', node_type:'T', parameters:{ _from_template:true }, children:[
          { name:'', node_type:'div', parameters:{}, is_hierarchy_transparent:true, children:[
            { name:'A', node_type:'int', parameters:{ value:{ Integer:1 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
          ]},
          { name:'B', node_type:'int', parameters:{ value:{ Integer:2 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
          { name:'C', node_type:'int', parameters:{ value:{ Formula:'A*B' } }, children:[], is_hierarchy_transparent:false },
        ], is_hierarchy_transparent:false },
      ], is_hierarchy_transparent:false }
    ], is_hierarchy_transparent:false }
  ]
}

function getFieldValueText(path){
  const el = findByPath(path)
  if (!el) return ''
  const val = el.querySelector('.field-value, .text-content') || el
  return (val.textContent||'').trim()
}

async function simulateEditA(app, path){
  const aEl = findByPath(path)
  expect(aEl).toBeTruthy()
  const span = aEl.querySelector('.field-value') || aEl
  // Simulate edit by invoking the renderer’s editing API directly via dblclick -> input -> blur
  span.dispatchEvent(new MouseEvent('dblclick', { bubbles:true }))
  // There should be an input inserted after dblclick
  const input = span.parentElement.querySelector('input.field-editor, textarea.field-editor')
  expect(input).toBeTruthy()
  input.value = '5'
  // Blur to commit
  input.dispatchEvent(new FocusEvent('blur', { bubbles:true }))
  // Allow microtask queue to flush (reevaluateDocumentSelective is async)
  await new Promise(r => setTimeout(r, 0))
}

describe('unnamed wrapper + label + list UI updates', () => {
  beforeEach(() => setupDOM())

  it('A under unnamed wrapper with label inside list: editing A updates C', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    let currentDoc = buildDoc_withUnnamedWrapperAndLabel()

    // Backend stubs: recompute C for both full and selective paths (post-fix behavior)
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') { const doc = deepClone(currentDoc); computeCProductForFirstListItem(doc); return Promise.resolve(doc) }
      if (cmd === 'parse_overseer_content_selective') { const doc = deepClone(currentDoc); computeCProductForFirstListItem(doc); return Promise.resolve(doc) }
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

    // Edit L/T__1/A to 5
    await simulateEditA(app, ['Root','L','T__1','A'])

    // Check C shows 10
    const cText = getFieldValueText(['Root','L','T__1','C'])
    expect(cText).toBe('10')
  })

  it('No unnamed wrapper: editing A updates C (expected PASS even before fix)', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    let currentDoc = buildDoc_withoutUnnamedWrapper()

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      // For this scenario, both selective and full recompute C
      if (cmd === 'parse_overseer_content') { const doc = deepClone(currentDoc); computeCProductForFirstListItem(doc); return Promise.resolve(doc) }
      if (cmd === 'parse_overseer_content_selective') { const doc = deepClone(currentDoc); computeCProductForFirstListItem(doc); return Promise.resolve(doc) }
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

    await simulateEditA(app, ['Root','L','T__1','A'])

    const cText = getFieldValueText(['Root','L','T__1','C'])
    expect(cText).toBe('10')
  })

  it('Unnamed wrapper but no label: editing A updates C (expected PASS even before fix)', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    let currentDoc = buildDoc_withoutLabel()

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      // For this scenario, both selective and full recompute C
      if (cmd === 'parse_overseer_content') { const doc = deepClone(currentDoc); computeCProductForFirstListItem(doc); return Promise.resolve(doc) }
      if (cmd === 'parse_overseer_content_selective') { const doc = deepClone(currentDoc); computeCProductForFirstListItem(doc); return Promise.resolve(doc) }
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

    await simulateEditA(app, ['Root','L','T__1','A'])

    const cText = getFieldValueText(['Root','L','T__1','C'])
    expect(cText).toBe('10')
  })
})
