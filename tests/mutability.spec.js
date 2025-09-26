import { describe, it, expect, beforeEach } from 'vitest'
import { bootstrapMinimalDom } from './helpers/dom'

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

describe('mutability parameter', () => {
  beforeEach(() => {
    // jsdom fresh DOM per test handled by setup
  })

  it('A=false blocks edits; B=true persists; C=guarded UI-only', async () => {
    const window = bootstrapMinimalDom()
    const { OverseerApp } = await import('../src/main.js')
    const app = new OverseerApp()
    window.app = app

    // Construct a minimal document with a tab containing A, B, C
    const doc = [
      { name: 'main', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: [
        { name: 'A', node_type: 'int', parameters: { value: { Integer: 15 }, mutable: { Boolean: false } }, children: [], is_hierarchy_transparent: false },
        { name: 'B', node_type: 'int', parameters: { value: { Integer: 10 }, mutable: { Boolean: true } }, children: [], is_hierarchy_transparent: false },
        { name: 'C', node_type: 'int', parameters: { value: { Integer: 55 }, mutable: { String: 'guarded' } }, children: [], is_hierarchy_transparent: false },
      ]}
    ]
    app._adoptDocumentPreserveRoot(JSON.parse(JSON.stringify(doc)))
    app.renderer.renderDocument(app.currentDocument)

    // A=false: dblclick should not create input
    const aEl = getFieldDisplayEndingWith('A')
    expect(aEl).toBeTruthy()
    aEl.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    let input = document.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeFalsy()

    // B=true: edit persists in document
    const bEl = getFieldDisplayEndingWith('B')
    expect(bEl).toBeTruthy()
    bEl.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    input = document.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeTruthy()
    input.value = '42'
    input.dispatchEvent(new window.Event('blur', { bubbles: true }))

    // Verify in-memory document updated
    const bNode = app.renderer.findNodeByPath(app.currentDocument, ['main','B'])
    const bVal = bNode?.parameters?.value
    const bNum = (bVal && (bVal.Integer ?? parseInt(bVal.String || bVal.Float || bVal, 10)))
    expect(bNum).toBe(42)

    // C=guarded: UI can change, but normalization before save should NOT persist change
    const cEl = getFieldDisplayEndingWith('C')
    expect(cEl).toBeTruthy()
    cEl.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    input = document.querySelector('input.field-editor, textarea.field-editor')
    expect(input).toBeTruthy()
    input.value = '99'
    input.dispatchEvent(new window.Event('blur', { bubbles: true }))

    // Normalize for serialization and assert original value is kept
    const normalized = app.normalizeDocumentForSerialization(JSON.parse(JSON.stringify(app.currentDocument)))
    const cNode = app.renderer.findNodeByPath(normalized, ['main','C'])
    const cVal = cNode?.parameters?.value
    const cNum = (cVal && (cVal.Integer ?? parseInt(cVal.String || cVal.Float || cVal, 10)))
    expect(cNum).toBe(55)
  })
})
