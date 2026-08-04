import { describe, it, expect, beforeEach, vi } from 'vitest'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { bootstrapMinimalDom } from './helpers/dom'

// Utility: find the visual display element for a field by trailing name in data-path
function getFieldDisplayEndingWith(name) {
  const candidates = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of candidates) {
    try {
      const p = JSON.parse(el.dataset.path || '[]')
      if (Array.isArray(p) && p[p.length - 1] === name) {
        return el.querySelector('.field-value, .overseer-list-value, .text-content') || el
      }
    } catch (_) {}
  }
  return null
}

describe('mutable=guarded does not persist after save', () => {
  beforeEach(() => {
    bootstrapMinimalDom()
  })

  it('edit C (guarded) then save -> serialized content keeps original C', async () => {
  // Reset module registry and configure the tauri invoke mock BEFORE importing the app
  await vi.resetModules()
  const { invoke } = await import('@tauri-apps/api/core')

    // Simulate a document equivalent to examples/basic/mutability_test.os
    // int A (mutable=true) = 55
    // int B (mutable=false) = 10
    // int C (mutable=guarded) = 66
    const ast = [
      { name: 'main', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: [
        { name: 'A', node_type: 'int', parameters: { value: { Integer: 55 }, mutable: { Boolean: true } }, children: [], is_hierarchy_transparent: false },
        { name: 'B', node_type: 'int', parameters: { value: { Integer: 10 }, mutable: { Boolean: false } }, children: [], is_hierarchy_transparent: false },
        { name: 'C', node_type: 'int', parameters: { value: { Integer: 66 }, mutable: { String: 'guarded' } }, children: [], is_hierarchy_transparent: false },
      ]}
    ]

    // Capture serialized content passed to the backend
    let lastSerialized = null
    const ORIGINAL_TEXT = 'main { A = 55 B = 10 C = 66 }' // dummy
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'parse_overseer_content') return Promise.resolve(ast)
      if (cmd === 'serialize_overseer_nodes') {
        // Capture the nodes payload after normalization
        lastSerialized = args.nodes
        // Return a basic serialization string (not used for assertion)
        return Promise.resolve('dummy serialization')
      }
      if (cmd === 'load_overseer_file') return Promise.resolve(ORIGINAL_TEXT)
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'execute_overseer_event') return Promise.resolve(null)
      if (cmd === 'save_overseer_file_with_original') return Promise.resolve(null)
      if (cmd === 'save_overseer_file') return Promise.resolve(null)
      return Promise.resolve(null)
    })

  const { OverseerApp } = await import('../src/main.js')

  const app = new OverseerApp()
    // Adopt directly instead of loadFile to avoid I/O paths
    app._adoptDocumentPreserveRoot(JSON.parse(JSON.stringify(ast)))
    app.renderer.renderDocument(app.currentDocument)

    // Use the dedicated save hook to capture the exact nodes passed into serialization
    app._testHook_beforeSerialize = (nodes) => {
      try { lastSerialized = JSON.parse(JSON.stringify(nodes)) } catch { lastSerialized = nodes }
    }

    // Change C via UI: double-click to edit and blur with new value (guarded should allow UI change)
    const cEl = getFieldDisplayEndingWith('C')
    expect(cEl).toBeTruthy()
    cEl.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    const input = document.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeTruthy()
    input.value = '999'
    input.dispatchEvent(new window.Event('blur', { bubbles: true }))

  // We capture via the tauri invoke mock above; no need to wrap normalizeDocumentForSerialization

    // Go through the real save path to ensure we exercise pending-edit merge and any adoption steps
  app.currentFile = 'dummy.os'
  app._originalText = ORIGINAL_TEXT
  await app.saveFile()

    // Find C in the serialized nodes payload and verify its value remains 66
    if (!lastSerialized) {
      // Fallback: directly normalize the current document (should produce the same nodes passed to serialize)
      const clone = JSON.parse(JSON.stringify(app.currentDocument))
      try { lastSerialized = app.normalizeDocumentForSerialization(clone) } catch(_) {}
    }
    function findByPath(doc, pathArr) {
      const find = (nodes, parts, idx=0) => {
        for (const n of nodes) {
          if (!n || typeof n !== 'object') continue
          if (n.name === parts[idx]) {
            if (idx === parts.length - 1) return n
            return find(n.children || [], parts, idx+1)
          }
        }
        return null
      }
      return Array.isArray(doc) ? find(doc, pathArr) : null
    }

    expect(lastSerialized).toBeTruthy()
    const cNode = findByPath(lastSerialized, ['main','C'])
    expect(cNode).toBeTruthy()
    const v = cNode.parameters && cNode.parameters.value
    // Value must remain original 66 (guarded UI changes are stripped or restored by normalization)
    const num = v && (v.Integer ?? parseInt(v.String || v.Float || v, 10))
    expect(num).toBe(66)
  })
})
