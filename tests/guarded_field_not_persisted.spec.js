import { describe, it, expect, beforeEach, vi } from 'vitest'

// `mutable="guarded"` means a field can be changed in the open document but the change is
// not written to disk. The tracker's `selected_date` relies on it: navigating days must not
// overwrite the authored `$(today())`, or the document stops defaulting to today.
//
// Day navigation goes through a `set` action rather than a field edit, so this checks the
// action path specifically - what reaches `serialize_overseer_nodes` after the backend has
// changed a guarded field.

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

function deepClone(o) { return JSON.parse(JSON.stringify(o)) }

function buildDoc() {
  return [{
    name: 'tracker', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false,
    source_id: 's1n1', source_fingerprint: 1, param_order: [], authored_dash: false,
    children: [{
      name: 'Selected', node_type: 'div', parameters: {}, is_hierarchy_transparent: false,
      source_id: 's1n2', source_fingerprint: 2, param_order: [], authored_dash: false,
      children: [
        {
          name: 'selected_date', node_type: 'timestamp',
          parameters: {
            precision: { String: 'day' },
            mutable: { String: 'guarded' },
            value: { Formula: 'today()' },
            _computed_value: { String: '2026-08-06' },
          },
          is_hierarchy_transparent: false,
          source_id: 's1n3', source_fingerprint: 3, param_order: [], authored_dash: false,
          children: [],
        },
        {
          name: 'Prev', node_type: 'button',
          parameters: { label: { String: '< Prev Day' } },
          is_hierarchy_transparent: false,
          source_id: 's1n4', source_fingerprint: 4, param_order: [], authored_dash: false,
          children: [{
            name: 'click', node_type: 'on', parameters: {}, is_hierarchy_transparent: false,
            children: [{
              name: 'set', node_type: 'set',
              parameters: { path: { String: '/tracker/Selected/selected_date' } },
              children: [], is_hierarchy_transparent: false,
            }],
          }],
        },
      ],
    }],
  }]
}

function findByName(nodes, name) {
  for (const n of nodes || []) {
    if (n.name === name) return n
    const f = findByName(n.children, name)
    if (f) return f
  }
  return null
}

describe('a guarded field changed by an action', () => {
  beforeEach(() => setupDOM())

  async function navigateAndSave(app, clicks) {
    const prevEl = Array.from(document.querySelectorAll('[data-path]')).find(el => {
      try { return JSON.parse(el.dataset.path || '[]').slice(-1)[0] === 'Prev' } catch { return false }
    })
    expect(prevEl, 'the Prev button should render').toBeTruthy()
    const btn = prevEl.matches('button') ? prevEl : prevEl.querySelector('button')
    for (let i = 0; i < clicks; i++) {
      btn.dispatchEvent(new Event('click', { bubbles: true }))
      await new Promise(r => setTimeout(r, 30))
    }
    let sent = null
    app._testHook_beforeSerialize = (nodes) => { sent = deepClone(nodes) }
    await app.saveFile()
    return sent
  }

  it('is not persisted, so the authored formula survives the save', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms' || cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'execute_overseer_event') {
        // What the backend does for `set`: writes the new value onto the node.
        const doc = deepClone(args.nodes)
        const node = findByName(doc, 'selected_date')
        node.parameters.value = { String: '2026-08-05' }
        node.parameters._computed_value = { String: '2026-08-05' }
        return Promise.resolve(doc)
      }
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

    // Navigate a day, exactly as pressing Prev does.
    const prevEl = Array.from(document.querySelectorAll('[data-path]')).find(el => {
      try { return JSON.parse(el.dataset.path || '[]').slice(-1)[0] === 'Prev' } catch { return false }
    })
    expect(prevEl, 'the Prev button should render').toBeTruthy()
    const btn = prevEl.matches('button') ? prevEl : prevEl.querySelector('button')
    btn.dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 30))

    // The in-memory document may hold the navigated date - that is the point of guarded.
    let sent = null
    app._testHook_beforeSerialize = (nodes) => { sent = deepClone(nodes) }
    await app.saveFile()

    expect(sent, 'the save path should serialize the document').toBeTruthy()
    const saved = findByName(sent, 'selected_date')
    expect(saved, 'the field should still be present').toBeTruthy()
    expect(
      saved.parameters.value,
      `a guarded field must be written as authored, got ${JSON.stringify(saved.parameters.value)}`
    ).toEqual({ Formula: 'today()' })
  })

  it('survives repeated changes, not just the first', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    // Each click steps the date back a day, as the real `set` action does.
    const dates = ['2026-08-05', '2026-08-04', '2026-08-03']
    let step = 0
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms' || cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'execute_overseer_event') {
        const doc = deepClone(args.nodes)
        const node = findByName(doc, 'selected_date')
        const next = dates[Math.min(step++, dates.length - 1)]
        node.parameters.value = { String: next }
        node.parameters._computed_value = { String: next }
        return Promise.resolve(doc)
      }
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

    const sent = await navigateAndSave(app, 2)
    const saved = findByName(sent, 'selected_date')
    expect(
      saved.parameters.value,
      `after two changes the authored value must still be restored, not the intermediate one - got ${JSON.stringify(saved.parameters.value)}`
    ).toEqual({ Formula: 'today()' })
  })
})
