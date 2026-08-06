import { describe, it, expect, beforeEach, vi } from 'vitest'

// Editing a field kicks off a backend round trip that can take a while on a large document.
// Two things must hold regardless of how long it takes:
//
//   - the field shows what was just typed, immediately, rather than reverting to its old
//     value until the response lands;
//   - edits made while a response is in flight survive it, instead of being overwritten by a
//     document the backend computed from the pre-edit state.
//
// Both are correctness problems rather than speed problems: they would still be wrong at
// 50 ms, just harder to notice.

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

const field = (name, value) => ({
  name, node_type: 'string',
  parameters: { mutable: { Boolean: true }, value: { String: value } },
  is_hierarchy_transparent: false, children: [],
  source_id: `s1n_${name}`, source_fingerprint: 1, param_order: [], authored_dash: false,
})

function buildDoc() {
  return [{
    name: 'doc', node_type: 'tab',
    parameters: { mutable: { Boolean: true } },
    is_hierarchy_transparent: false,
    source_id: 's1n1', source_fingerprint: 1, param_order: [], authored_dash: false,
    children: [field('alpha', 'one'), field('beta', 'two')],
  }]
}

function elementFor(name) {
  return Array.from(document.querySelectorAll('[data-path]')).find(el => {
    try { return JSON.parse(el.dataset.path || '[]').slice(-1)[0] === name } catch { return false }
  })
}

function shownValue(name) {
  const el = elementFor(name)
  if (!el) return null
  const holder = el.querySelector('.field-value, .text-content') || el
  return (holder.textContent || '').trim()
}

/** Type into a field and blur, without waiting for the round trip to settle. */
function editField(name, text) {
  const el = elementFor(name)
  const holder = el.querySelector('.field-value, .text-content') || el
  holder.dispatchEvent(new Event('dblclick', { bubbles: true }))
  const input = document.querySelector('input.field-editor, textarea.field-editor')
  input.value = text
  input.dispatchEvent(new Event('blur'))
}

function docValue(app, name) {
  const node = app.renderer.findNodeByPath(app.currentDocument, ['doc', name])
  const v = node && node.parameters && node.parameters.value
  return v && v.String !== undefined ? v.String : v
}

/**
 * Backend stub with a controllable delay. The selective re-resolve answers from whatever the
 * frontend sent at the time of the call, which is what makes a slow response able to clobber
 * a newer edit.
 */
function installBackend(delayMs) {
  installBackend.call_index = 0
  return async (cmd, args) => {
    if (cmd === 'get_next_timer_due_ms' || cmd === 'scheduler_tick') return null
    if (cmd === 'serialize_overseer_nodes') {
      installBackend.lastSent = deepClone(args.nodes)
      return 'DOC'
    }
    if (cmd === 'parse_overseer_content_selective' || cmd === 'parse_overseer_content') {
      // Answer from what the frontend sent at the time of this call, as the backend does.
      const answer = deepClone(installBackend.lastSent || [])
      const wait = typeof delayMs === 'function' ? delayMs(installBackend.call_index++) : delayMs
      if (wait > 0) await new Promise(r => setTimeout(r, wait))
      return answer
    }
    if (cmd === 'execute_overseer_event') return deepClone(args.nodes)
    return null
  }
}

describe('editing a field while the backend is busy', () => {
  beforeEach(() => setupDOM())

  it('shows the typed value immediately, not the old one', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(installBackend(80))

    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.renderer.renderDocument(app.currentDocument)
    expect(shownValue('alpha')).toBe('one')

    editField('alpha', 'edited')
    // Deliberately before the round trip settles: this is the window the user sees.
    await new Promise(r => setTimeout(r, 10))

    expect(
      shownValue('alpha'),
      'the field reverted to its old value while the backend was working'
    ).toBe('edited')
  })

  it('does not discard an edit made while a response is in flight', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(installBackend(80))

    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.renderer.renderDocument(app.currentDocument)

    editField('alpha', 'first')
    await new Promise(r => setTimeout(r, 10))
    // Second edit lands while the first round trip is still outstanding.
    editField('beta', 'second')

    await new Promise(r => setTimeout(r, 300))

    expect(docValue(app, 'alpha'), 'the first edit was lost').toBe('first')
    expect(docValue(app, 'beta'), 'the edit made during the round trip was lost').toBe('second')
    expect(shownValue('beta'), 'the second edit is not shown').toBe('second')
  })

  it('ignores a stale response that arrives after a newer one', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    // The first round trip is slow, the second quick, so the older answer lands last -
    // carrying a document that predates the second edit.
    invoke.mockImplementation(installBackend(i => (i === 0 ? 200 : 10)))

    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.renderer.renderDocument(app.currentDocument)

    editField('alpha', 'first')
    await new Promise(r => setTimeout(r, 10))
    editField('beta', 'second')

    await new Promise(r => setTimeout(r, 400))

    expect(docValue(app, 'beta'), 'a stale response overwrote a newer edit').toBe('second')
    expect(docValue(app, 'alpha'), 'the first edit was lost').toBe('first')
    expect(shownValue('beta'), 'the newer edit is not shown').toBe('second')
  })
})
