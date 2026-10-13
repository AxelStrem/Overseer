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

// Build a simple doc with a guarded int field and a button that triggers a backend action changing it
function buildDoc(){
  return [
    { name:'main', node_type:'tab', parameters:{}, children:[
      { name:'C', node_type:'int', parameters:{ value:{ Integer:66 }, mutable:{ String:'guarded' } }, children:[] },
      { name:'DoIt', node_type:'button', parameters:{ label:{ String:'do' } }, children:[
        { name:'click', node_type:'on', parameters:{}, children:[] }
      ] }
    ] }
  ]
}

// Helper to find element by path array
function findByPath(pathArray){
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p.length===pathArray.length && p.every((v,i)=>v===pathArray[i])) return el } catch(_){ }
  }
  return null
}

describe('mutable=guarded via action does not persist on save', () => {
  beforeEach(() => setupDOM())

  it('button click that changes C (guarded) is UI-only and not saved', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    const originalDoc = buildDoc()
    let currentDoc = deepClone(originalDoc)

    let saved = null

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'execute_overseer_event_with_text') {
        // Simulate backend action: set main/C to 99
        const newDoc = deepClone(currentDoc)
        newDoc.find(n=>n.name==='main').children.find(n=>n.name==='C').parameters.value = { Integer: 99 }
        return Promise.resolve({ nodes: newDoc, text: 'DOC-WITH-C-99' })
      }
      if (cmd === 'save_overseer_file_from_text') { saved = args; return Promise.resolve(null) }
      if (cmd === 'load_overseer_file') return Promise.resolve('DOC')
      return Promise.reject(new Error('unknown command: '+cmd))
    })

    const app = new OverseerApp()
    app.currentDocument = deepClone(currentDoc)
    app._currentText = 'DOC'
    app._originalText = 'DOC'
    app.renderer.renderDocument(app.currentDocument)

    // Click the button to trigger action
    const btnEl = findByPath(['main','DoIt'])
    expect(btnEl).toBeTruthy()
    btnEl.click()
    // Wait a tick for async emitEvent handler to complete
    await new Promise(r => setTimeout(r, 0))

    // After action, UI doc should show 99 for C but it must be tagged guarded for save
    const afterDoc = app.currentDocument
    const Cnode = afterDoc.find(n=>n.name==='main').children.find(n=>n.name==='C')
    expect(Cnode.parameters.value.Integer).toBe(99)

    app.currentFile = 'dummy.os'
    await app.saveFile()

    // The text resolved with C=99 goes with C's authored 66, to be put back before writing.
    expect(saved).toBeTruthy()
    expect(saved.content).toBe('DOC-WITH-C-99')
    const revert = (saved.guarded || []).find(g => g.path === 'main/C')
    expect(revert && revert.value).toEqual({ Integer: 66 })
  })
})
