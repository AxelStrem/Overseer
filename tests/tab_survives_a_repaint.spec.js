import { describe, it, expect, beforeEach, vi } from 'vitest'

// A tab is repainted whenever its own parameters change, and an event changes them every time
// it touches a list inside the tab - the resolver records the overrides on the tab node, so
// pressing `bought` on a shopping item reports a parameters change on `shopping` itself.
//
// Rendering a tab used to be append-only: a new button on the end of the row, and content that
// starts hidden and is only revealed when it is the only tab. Repainting one therefore left a
// second button for the same tab and swapped the visible page for a hidden one. What you saw
// was the page going blank, with a new empty tab of the same name beside it.

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

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const tab = (name, label, overrides) => Object.assign(base(name, 'tab'), {
  parameters: {
    label: { String: label },
    mutable: { Boolean: true },
    ...(overrides ? { _explicit_overrides: { String: overrides } } : {}),
  },
  children: [Object.assign(base('milk', 'string'), { parameters: { value: { String: 'Milk' } } })],
})

const buttons = () => Array.from(document.querySelectorAll('.tab-button'))
const visible = () => Array.from(document.querySelectorAll('.tab-content'))
  .filter(c => c.style.display !== 'none')

describe('repainting a tab', () => {
  let app

  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
    app.currentDocument = [tab('shopping', 'Shopping')]
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  })

  it('starts with one tab, showing', () => {
    expect(buttons().map(b => b.textContent)).toEqual(['Shopping'])
    expect(visible().length).toBe(1)
  })

  it('does not grow a second tab of the same name', () => {
    // What an event does: the tab's parameters change, so the tab is repainted.
    app.currentDocument = [tab('shopping', 'Shopping', 'History,List')]
    app.renderer.rerenderSubtree(app.currentDocument, ['shopping'])

    expect(
      buttons().map(b => b.textContent),
      'the repaint left a duplicate tab beside the original'
    ).toEqual(['Shopping'])
  })

  it('leaves the page showing what it was showing', () => {
    app.currentDocument = [tab('shopping', 'Shopping', 'History,List')]
    app.renderer.rerenderSubtree(app.currentDocument, ['shopping'])

    expect(visible().length, 'the page went blank').toBe(1)
    expect(visible()[0].textContent, 'the visible tab is not the one with the content').toContain('Milk')
    expect(buttons()[0].classList.contains('active'), 'no tab is marked active').toBe(true)
  })

  it('keeps the tab that was open when there are several', () => {
    app.currentDocument = [tab('shopping', 'Shopping'), tab('tasks', 'Tasks')]
    app.renderer.renderDocument(app.currentDocument)
    buttons()[1].dispatchEvent(new Event('click', { bubbles: true }))
    expect(buttons()[1].classList.contains('active')).toBe(true)

    app.currentDocument = [tab('shopping', 'Shopping'), tab('tasks', 'Tasks', 'Open')]
    app.renderer.rerenderSubtree(app.currentDocument, ['tasks'])

    expect(buttons().length, 'a duplicate tab appeared').toBe(2)
    expect(buttons()[1].classList.contains('active'), 'the repaint moved you to another tab').toBe(true)
    expect(visible().length).toBe(1)
  })
})
