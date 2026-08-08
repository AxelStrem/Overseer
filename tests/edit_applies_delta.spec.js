import { describe, it, expect, beforeEach, vi } from 'vitest'

// An edit is answered with what changed rather than with the document. Two things have to
// hold for that to be worth doing: the change has to land in the document the app holds, and
// only the affected part of the page may be repainted - rebuilding all 15,000 elements is
// most of what an interaction costs on a large document.

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

const field = (name, value) => ({
  name, node_type: 'string',
  parameters: { mutable: { Boolean: true }, value: { String: value } },
  is_hierarchy_transparent: false, children: [],
  source_id: `s1n_${name}`, source_fingerprint: 1, param_order: [], authored_dash: false,
})

const buildDoc = () => ([{
  name: 'doc', node_type: 'tab',
  parameters: { mutable: { Boolean: true } },
  is_hierarchy_transparent: false,
  source_id: 's1n1', source_fingerprint: 1, param_order: [], authored_dash: false,
  children: [field('alpha', 'one'), field('beta', 'two')],
}])

function editField(name, text) {
  const el = Array.from(document.querySelectorAll('[data-path]')).find(e => {
    try { return JSON.parse(e.dataset.path || '[]').slice(-1)[0] === name } catch { return false }
  })
  const holder = el.querySelector('.field-value, .text-content') || el
  holder.dispatchEvent(new Event('dblclick', { bubbles: true }))
  const input = document.querySelector('input.field-editor, textarea.field-editor')
  input.value = text
  input.dispatchEvent(new Event('blur'))
}

describe('an edit answered with a change', () => {
  let app, fullRenders, subtreeRepaints

  beforeEach(async () => {
    setupDOM()
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(async (cmd) => {
      if (cmd === 'parse_overseer_content_selective_update') {
        return {
          text: 'TEXT-AFTER',
          // 'beta' is a dependent the edit recomputed - exactly the case a whole document
          // would otherwise be sent for.
          changes: [
            { kind: 'parameters', address: 'doc/beta', path: [0, 1],
              parameters: { mutable: { Boolean: true }, value: { String: 'recomputed' } } },
          ],
          nodes: null,
        }
      }
      return null
    })

    app = new OverseerApp()
    app.currentDocument = buildDoc()
    app._currentText = 'TEXT-BEFORE'
    app.renderer.renderDocument(app.currentDocument)

    fullRenders = 0
    subtreeRepaints = 0
    const render = app.renderer.renderDocument.bind(app.renderer)
    app.renderer.renderDocument = (d) => { fullRenders++; return render(d) }
    const sub = app.renderer.rerenderSubtree.bind(app.renderer)
    app.renderer.rerenderSubtree = (d, p) => { subtreeRepaints++; return sub(d, p) }
  })

  it('lands the change in the document and keeps the returned text', async () => {
    editField('alpha', 'edited')
    await new Promise(r => setTimeout(r, 50))

    const beta = app.currentDocument[0].children[1]
    expect(beta.parameters.value, 'the described change did not reach the document').toEqual({ String: 'recomputed' })
    expect(app._currentText, 'the text for the next interaction was not kept').toBe('TEXT-AFTER')
  })

  it('repaints the changed node instead of the whole document', async () => {
    editField('alpha', 'edited')
    await new Promise(r => setTimeout(r, 50))

    expect(subtreeRepaints, 'the changed node was not repainted in place').toBeGreaterThan(0)
    expect(
      fullRenders,
      'the document was rebuilt, which is the cost this exists to avoid'
    ).toBe(0)
  })

  it('shows the recomputed value on screen', async () => {
    editField('alpha', 'edited')
    await new Promise(r => setTimeout(r, 50))

    const el = Array.from(document.querySelectorAll('[data-path]')).find(e => {
      try { return JSON.parse(e.dataset.path || '[]').slice(-1)[0] === 'beta' } catch { return false }
    })
    const shown = ((el.querySelector('.field-value, .text-content') || el).textContent || '').trim()
    expect(shown, 'the page still shows the old value').toContain('recomputed')
  })
})
