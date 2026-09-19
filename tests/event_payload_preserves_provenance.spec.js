import { describe, it, expect, beforeEach, vi } from 'vitest'

// The backend serializer is snapshot-driven: it replays each node's original source text,
// looked up in the SourceRegistry by `source_id`. That id is the only handle the frontend
// carries - the snapshot itself is #[serde(skip)] and never crosses the IPC boundary.
//
// Whatever `execute_overseer_event` returns is adopted as the live document, so a node that
// loses its provenance on the way out has lost it for every subsequent save. That is not a
// cosmetic loss: without it the serializer falls back to reformatting from the AST, which
// drops comments, rewrites explicit type keywords (`int p1 = 1` -> `- p1 = 1`) and emits
// empty bodies for template and action blocks.

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

// A document shaped like a parsed one: every node carries the provenance the backend needs.
function buildDoc() {
  let seq = 0
  const withProvenance = (node) => {
    seq += 1
    node.source_id = `ns${seq}`
    node.source_fingerprint = 1000 + seq
    node.param_order = Object.keys(node.parameters || {})
    node.authored_dash = false
    node.child_original_index = seq
    node.leading_blank_lines = 0
    node.raw_value_literal = null
    if (!Array.isArray(node.children)) node.children = []
    node.children.forEach(withProvenance)
    return node
  }

  return [withProvenance({
    name: 'tracker', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
      {
        name: 'Exercise', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
          { name: 'id', node_type: 'int', parameters: { value: { Integer: 1 } }, children: [], is_hierarchy_transparent: false },
          {
            name: 'plates', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
              { name: 'p1', node_type: 'int', parameters: { value: { Integer: 1 } }, children: [], is_hierarchy_transparent: false },
            ]
          },
          {
            name: 'done', node_type: 'button', parameters: { label: { String: 'Done' } }, is_hierarchy_transparent: false, children: [
              {
                name: 'click', node_type: 'on', parameters: {}, is_hierarchy_transparent: false, children: [
                  { name: 'set_now_ts', node_type: 'set_now_ts', parameters: { path: { String: '/tracker/Exercise/id' } }, children: [], is_hierarchy_transparent: false },
                ]
              },
            ]
          },
        ]
      },
    ]
  })]
}

function collectMissingProvenance(nodes, trail = 'root', missing = []) {
  for (const n of nodes || []) {
    const where = `${trail}/${n.name || '(unnamed)'}`
    if (n.source_id === undefined) missing.push(`${where}: source_id`)
    if (n.param_order === undefined) missing.push(`${where}: param_order`)
    if (n.authored_dash === undefined) missing.push(`${where}: authored_dash`)
    collectMissingProvenance(n.children, where, missing)
  }
  return missing
}

function findNode(nodes, name) {
  for (const n of nodes || []) {
    if (n.name === name) return n
    const f = findNode(n.children, name)
    if (f) return f
  }
  return null
}

describe('event payload preserves node provenance', () => {
  beforeEach(() => setupDOM())

  it('sends source_id and authored formatting metadata to execute_overseer_event', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    const eventCalls = []
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms' || cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'execute_overseer_event') {
        eventCalls.push(deepClone(args))
        return Promise.resolve(deepClone(args.nodes))
      }
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      return Promise.resolve(null)
    })

    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.renderer.renderDocument(app.currentDocument)

    const doneEl = Array.from(document.querySelectorAll('[data-path]')).find(el => {
      try { return JSON.parse(el.dataset.path || '[]').join('/') === 'tracker/Exercise/done' } catch { return false }
    })
    expect(doneEl, 'the done button should render').toBeTruthy()

    const btn = doneEl.querySelector('button') || doneEl
    btn.dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 20))

    expect(eventCalls.length, 'clicking should emit an event to the backend').toBeGreaterThan(0)

    const sent = eventCalls[0].nodes
    const missing = collectMissingProvenance(sent)
    expect(missing, `provenance dropped before reaching the backend:\n${missing.join('\n')}`).toEqual([])

    // The specific handle the serializer looks the source snapshot up by.
    expect(findNode(sent, 'p1').source_id).toBe(findNode(app.currentDocument, 'p1').source_id)

    // Action bodies must survive too: an emptied `on click` block is exactly how the
    // `button done { }` corruption showed up on disk.
    const sentClick = findNode(sent, 'click')
    expect(sentClick, 'the on-click block should be sent').toBeTruthy()
    expect(sentClick.children.length, 'action body should not be emptied').toBeGreaterThan(0)
  })
})
