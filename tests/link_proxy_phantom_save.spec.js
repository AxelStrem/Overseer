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
import { answerEnsureEntry } from './helpers/ensure-entry.js'

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

// This used to end by saving the document and checking that every node in what went up carried
// `is_hierarchy_transparent` - a field the page once left off an entry it built itself, which
// broke serializing the document that held it.
//
// Two things have changed since. The entry comes from the backend rather than from the page, and
// the wire deliberately leaves out any field sitting at its default, filling it in again on the
// way back - so its absence is correct and checking for it tests the wire format rather than this.
// And the change is written as part of being made, so there is no save afterwards to check.
describe('a preview made real reaches the file without the document being saved', () => {
  beforeEach(() => setupDOM())

  it('makes the entry, shows it in the list, and leaves no save behind', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    // Track last serialized nodes to assert schema
    let lastSerialized = null
    invoke.mockImplementation((cmd, args) => {
      // Making a preview real is a backend instruction now; the page used to do it itself in
      // its own copy of the document. See `helpers/ensure-entry.js`.
      const madeReal = answerEnsureEntry(() => app, cmd, args)
      if (madeReal !== null) return madeReal
      if (cmd === 'serialize_overseer_nodes') { lastSerialized = deepClone(args.nodes); currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      // Answered from the document as it now stands, not from a copy captured the last time
      // the page serialized one: a change that goes as an instruction serializes nothing, so
      // such a copy predates the write and answering with it would undo it.
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(app.currentDocument))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(app.currentDocument))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('')
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    // Opened from somewhere, which every document a view is edited through is:
    // making a preview real is a write, and a write needs a file to reach.
    app.currentFile = '/documents/test.os'
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
    await (async () => { for (let i = 0; i < 20; i += 1) await new Promise(r => setTimeout(r, 0)) })()  // making a preview real is a round trip now, so one tick is not enough
    // No full re-resolve here. Making the preview real went as an instruction and
    // its answer carried the resolved subtree; asking for the whole document again
    // would have this fake answer from the copy it captured before the write.

    // Verify item created in memory
    const root = app.currentDocument.find(n=>n.name==='Root')
    const list = root.children.find(n=>n.name==='Tasks')
    const created = list.children.find(it => it.children.some(f=>f.name==='id' && (f.parameters.value?.String||f.parameters.value)==='s1'))
    expect(created).toBeTruthy()

    // It carries the edit that asked for it.
    const title = created.children.find((c) => c.name === 'title')
    expect(title?.parameters?.value?.String ?? title?.parameters?.value).toBe('Saved Title')

    // And a save has nothing left to send: it was written as part of being made. That is the
    // whole point - a change that goes up as a whole document lands on top of whatever wrote in
    // the meantime, and this was the last page change that did.
    await app.saveFile()
    expect(lastSerialized, 'the document went up as text as well').toBeNull()
  })
})
