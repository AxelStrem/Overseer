import { describe, it, expect, beforeEach, vi } from 'vitest'

// A mount's declaration must survive a plain load-and-save with nothing edited. The backend
// preserves it - the serializer replays the node's authored text from its snapshot, keyed by
// `source_id` and gated on `source_fingerprint`. So if the braces vanish in the app, the
// document handed to the serializer has lost that provenance somewhere on the frontend.
//
// This checks what actually reaches `serialize_overseer_nodes`, via the save path's own hook.

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

function deepClone(o) { return JSON.parse(JSON.stringify(o)) }

// A mount as it arrives from the backend after loading: provenance intact, and holding the
// children that preloading materialized.
function buildDoc() {
  return [{
    name: 'tracker', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false,
    source_id: 's1n1', source_fingerprint: 1001, param_order: [], authored_dash: false,
    children: [
      {
        name: 'FOODS', node_type: 'mount',
        parameters: {
          hidden: { Boolean: true },
          lazy: { Boolean: false },
          source: { String: 'foods.os/food_catalog/Catalog' },
          _mount_status: { String: 'loaded' },
        },
        is_hierarchy_transparent: false,
        source_id: 's1n2', source_fingerprint: 1002, param_order: [], authored_dash: false,
        children: [{
          name: 'Catalog', node_type: 'list', parameters: {}, is_hierarchy_transparent: false,
          source_id: 's2n1', source_fingerprint: 2001, param_order: [], authored_dash: false,
          children: [],
        }],
      },
      {
        name: 'note', node_type: 'string',
        parameters: { value: { String: 'hello' } },
        is_hierarchy_transparent: false,
        source_id: 's1n3', source_fingerprint: 1003, param_order: [], authored_dash: false,
        children: [],
      },
    ],
  }]
}

function findMount(nodes) {
  for (const n of nodes || []) {
    if (n.node_type === 'mount') return n
    const f = findMount(n.children)
    if (f) return f
  }
  return null
}

describe('a mount declaration on a plain load-and-save', () => {
  beforeEach(() => setupDOM())

  it('reaches the serializer with its provenance intact', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation((cmd) => {
      if (cmd === 'get_next_timer_due_ms' || cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      if (cmd === 'save_overseer_file' || cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'load_overseer_file') return Promise.resolve('DOC')
      return Promise.resolve(null)
    })

    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.currentFile = 'C:/tmp/tracker.os'
    app._originalText = 'DOC'
    app.renderer.renderDocument(app.currentDocument)

    let sent = null
    app._testHook_beforeSerialize = (nodes) => { sent = deepClone(nodes) }
    await app.saveFile()

    expect(sent, 'the save path should serialize the document').toBeTruthy()
    const mount = findMount(sent)
    expect(mount, 'the mount should still be in the document').toBeTruthy()

    expect(
      mount.source_id,
      'source_id identifies the snapshot the serializer replays the declaration from'
    ).toBe('s1n2')
    expect(
      mount.source_fingerprint,
      'without the fingerprint the serializer cannot tell the node is unchanged, and falls back to reformatting it'
    ).toBe(1002)
  })
})
