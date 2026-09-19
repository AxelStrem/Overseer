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

import { OverseerRenderer } from '../src/renderer.js'
import { OverseerApp } from '../src/main.js'

function findElementByPath(pathArray) {
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try {
      const p = JSON.parse(el.dataset.path || '[]')
      if (Array.isArray(p) && p.length === pathArray.length && p.every((v,i)=>v===pathArray[i])) return el
    } catch(_) {}
  }
  return null
}

describe('Phantom edit then save serializes with required schema', () => {
  beforeEach(() => setupDOM())

  it('materializes item, updates list UI, and saves without missing is_hierarchy_transparent', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    // Track last serialized nodes to assert schema
    let lastSerialized = null
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') { lastSerialized = deepClone(args.nodes); currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('')
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    // Document with list having entry template Task and key=id
    currentDoc = [
      { name: 'Task', node_type: 'div', parameters: {}, children: [
        { name: 'id', node_type: 'string', parameters: { value: { String: '' } }, children: [] },
        { name: 'title', node_type: 'string', parameters: { value: { String: '' }, mutable: { Boolean: true } }, children: [] }
      ], is_hierarchy_transparent: false },
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'Tasks', node_type: 'list', parameters: { entry: { Template: 'Task' }, key: { String: 'id' } }, children: [], is_hierarchy_transparent: false },
        { name: 'ItemView', node_type: 'div', parameters: { link: '/Root/Tasks[key="s1"]/title' }, children: [], is_hierarchy_transparent: false }
      ], is_hierarchy_transparent: false }
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

    // Initially phantom
    const linkContainer = findElementByPath(['Root','ItemView'])
    expect(linkContainer).toBeTruthy()
    expect(linkContainer.hasAttribute('data-link-phantom')).toBe(true)

    // Edit the title under the phantom to materialize
    const titleEl = linkContainer.querySelector('.overseer-field .field-value') || linkContainer.querySelector('.field-value')
    expect(titleEl).toBeTruthy()

    // Provide path-based updater fallback for tests
    renderer.updateNodeValueByPath = (doc, path, value) => {
      const find = (nodes, parts) => {
        let cur = { children: nodes }
        for (const seg of parts) {
          const [b,o] = seg.includes('#')?seg.split('#'):[seg,'0']
          const ms = cur.children.filter(c=>c.name===b); const idx=parseInt(o,10)
          if (idx>=ms.length) return null; cur=ms[idx]
        }
        return cur
      }
      const target = find(doc, path.split('/'))
      if (!target) return false
      if (!target.parameters) target.parameters = {}
      target.parameters.value = { String: value }
      return true
    }

    titleEl.dispatchEvent(new Event('dblclick', { bubbles: true }))
    const input = linkContainer.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeTruthy()
    input.value = 'Saved Title'
    input.dispatchEvent(new Event('blur'))
    await new Promise(r => setTimeout(r, 0))
    await app.reevaluateDocumentSelective([])

    // Verify item created in memory
    const root = app.currentDocument.find(n=>n.name==='Root')
    const list = root.children.find(n=>n.name==='Tasks')
    const created = list.children.find(it => it.children.some(f=>f.name==='id' && (f.parameters.value?.String||f.parameters.value)==='s1'))
    expect(created).toBeTruthy()

    // Save file – should invoke serialize without error
    await app.saveFile()
    expect(lastSerialized).toBeTruthy()

    // Assert all nodes have is_hierarchy_transparent defined
    const checkFlag = (node) => {
      expect(typeof node.is_hierarchy_transparent).toBe('boolean')
      if (Array.isArray(node.children)) node.children.forEach(checkFlag)
    }
    if (Array.isArray(lastSerialized)) lastSerialized.forEach(checkFlag)
    else checkFlag(lastSerialized)
  })
})
