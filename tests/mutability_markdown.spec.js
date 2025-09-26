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

describe('mutability parameter (markdown)', () => {
  beforeEach(() => {
    // fresh DOM handled in setup
  })

  it('Text with markdown: guarded edits are UI-only', async () => {
    const window = bootstrapMinimalDom()
    const { OverseerApp } = await import('../src/main.js')
    const app = new OverseerApp()
    window.app = app

    const doc = [
      { name: 'main', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: [
        { name: 'M', node_type: 'text', parameters: { value: { String: 'Hello' }, markdown: { Boolean: true }, mutable: { String: 'guarded' } }, children: [], is_hierarchy_transparent: false },
      ]}
    ]
    app._adoptDocumentPreserveRoot(JSON.parse(JSON.stringify(doc)))
    app.renderer.renderDocument(app.currentDocument)

    const mEl = getFieldDisplayEndingWith('M')
    expect(mEl).toBeTruthy()
    // Enable editing (markdown editor is opened via dblclick on text field)
    mEl.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    const textarea = document.querySelector('textarea.markdown-editor')
    expect(textarea).toBeTruthy()
    textarea.value = 'World'
    // Save via toolbar button
  const saveBtn = document.querySelector('.markdown-editor-container .save-btn')
    expect(saveBtn).toBeTruthy()
    saveBtn.click()

    // After save, normalize and expect original value persisted (guarded => UI-only)
    const normalized = app.normalizeDocumentForSerialization(JSON.parse(JSON.stringify(app.currentDocument)))
    const mNode = app.renderer.findNodeByPath(normalized, ['main','M'])
    const v = mNode?.parameters?.value
    const s = v && (v.String ?? (typeof v === 'string' ? v : null))
    expect(s).toBe('Hello')
  })
})
