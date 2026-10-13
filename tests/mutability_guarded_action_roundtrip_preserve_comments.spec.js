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

// Original file content (keep exactly, including blank lines and indentation)
const originalContent = `int A (mutable=true) = 55\nint B (mutable=false) = 10\nint C (mutable=\"guarded\") = 11\nint D = 6\n\ndiv dr {\n\n    button Prev (label=\"Update\") {\n        on click {\n            set (path=\"/dr/E\") = 20\n\n        }\n    }\n   \n   int E (mutable=\"guarded\") = 10\n   // comment line\n\n}\n\n`

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

import { OverseerApp } from '../src/main.js'

function deepClone(o){ return JSON.parse(JSON.stringify(o)) }
function findByPath(pathArray){
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p.length===pathArray.length && p.every((v,i)=>v===pathArray[i])) return el } catch(_){ }
  }
  return null
}

describe('guarded action roundtrip preserves original text including comment placement', () => {
  beforeEach(() => setupDOM())

  it('clicking Prev updates E in UI, and the save sends the text with the authored E', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    let currentDoc = null
    let saved = null

    // The text the backend answers the press with: E moved, everything else as authored.
    const afterPress = originalContent.replace('= 10\n   // comment', '= 20\n   // comment')

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve(originalContent)
      if (cmd === 'parse_overseer_content') {
        // A handcrafted AST equivalent to originalContent
        if (!currentDoc) {
          currentDoc = [
            { name:'A', node_type:'int', parameters:{ value:{ Integer:55 }, mutable:{ Boolean:true } }, children:[], is_hierarchy_transparent:false },
            { name:'B', node_type:'int', parameters:{ value:{ Integer:10 }, mutable:{ Boolean:false } }, children:[], is_hierarchy_transparent:false },
            { name:'C', node_type:'int', parameters:{ value:{ Integer:11 }, mutable:{ String:'guarded' } }, children:[], is_hierarchy_transparent:false },
            { name:'D', node_type:'int', parameters:{ value:{ Integer:6 } }, children:[], is_hierarchy_transparent:false },
            { name:'dr', node_type:'div', parameters:{}, is_hierarchy_transparent:false, children:[
              { name:'Prev', node_type:'button', parameters:{ label:{ String:'Update' } }, children:[
                { name:'click', node_type:'on', parameters:{}, children:[] }
              ], is_hierarchy_transparent:false },
              { name:'E', node_type:'int', parameters:{ value:{ Integer:10 }, mutable:{ String:'guarded' } }, children:[], is_hierarchy_transparent:false }
            ] }
          ]
        }
        return Promise.resolve(deepClone(currentDoc))
      }
      if (cmd === 'execute_overseer_event_with_text') {
        // The set action, /dr/E = 20, worked out from the text the page sent.
        expect(args.content).toBe(originalContent)
        const newDoc = deepClone(currentDoc)
        newDoc.find(n=>n.name==='dr').children.find(n=>n.name==='E').parameters.value = { Integer:20 }
        return Promise.resolve({ nodes: newDoc, text: afterPress })
      }
      if (cmd === 'save_overseer_file_from_text') { saved = args; return Promise.resolve(null) }
      return Promise.reject(new Error('Unknown command '+cmd))
    })

    const app = new OverseerApp()
    await app.loadFile('dummy_path.os')

    const btn = findByPath(['dr','Prev'])
    expect(btn).toBeTruthy()
    btn.click()
    await new Promise(r => setTimeout(r, 0))

    await app.saveFile()

    // The text keeps its comment, because nothing rebuilds it from the document; the
    // authored E travels alongside, for the backend to put back before writing.
    expect(saved).toBeTruthy()
    expect(saved.content).toBe(afterPress)
    expect(saved.content.includes('// comment line')).toBe(true)
    const revert = (saved.guarded || []).find(g => g.path === 'dr/E')
    expect(revert && revert.value).toEqual({ Integer: 10 })
  })
})
