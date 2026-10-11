import { describe, it, expect, vi } from 'vitest'

// `icon=` on a button names an icon of the library the build ships in `/icons`. The page draws it
// as one masked box in the text's colour; a name that is not a plain icon name draws nothing.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        </div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const draw = (buttons) => {
  setupDOM()
  const app = new OverseerApp()
  app.currentDocument = [base('t', 'tab', {}, buttons)]
  app.currentFile = 't.os'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

const button = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}'] button`)
  || document.querySelector(`button[data-path='${JSON.stringify(['t', name])}']`)

describe('a button draws a library icon', () => {
  it('names the icon by its address, and the button by its name', () => {
    draw([base('note', 'button', { icon: { String: 'note' } })])
    const b = button('note')
    const icon = b.querySelector('span.icon')
    expect(b.classList.contains('icon-button')).toBe(true)
    expect(icon.dataset.icon).toBe('note')
    expect(icon.style.getPropertyValue('--icon')).toBe('url("/icons/note.svg")')
    expect(b.title).toBe('note')
    expect(b.querySelector('[class*="glyph-"]')).toBeNull()
  })

  it('draws the icon in front of a label', () => {
    draw([base('add', 'button', { icon: { String: 'Plus' }, label: { String: 'Add' } })])
    const b = button('add')
    expect(b.classList.contains('icon-button')).toBe(false)
    expect(b.firstElementChild.dataset.icon).toBe('plus')
    expect(b.querySelector('.overseer-button-label').textContent).toBe('Add')
  })

  it('draws an empty slot for a name that is not a plain icon name', () => {
    draw([base('odd', 'button', { icon: { String: '../secret' } })])
    const icon = button('odd').querySelector('span.icon')
    expect(icon.dataset.icon).toBeUndefined()
    expect(icon.style.getPropertyValue('--icon')).toBe('')
  })
})
