import { describe, it, expect, beforeEach, vi } from 'vitest'

// How one kind of element looks is a key of the theme that holds keys of its own:
// `context theme { - checkbox { - style = switch } }` draws the bool and checkbox fields inside its
// container as switches. The page writes it as `--ov-checkbox-style` and, once a box is in the
// page, marks it from the nearest one above it.

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

const key = (name, value) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value } },
})

const kind = (name, keys) => ({
  name, node_type: '-', children: keys, is_hierarchy_transparent: false, parameters: {},
})

const theme = (keys) => ({
  name: 'theme', node_type: 'context', is_hierarchy_transparent: false, children: keys,
  parameters: { _context_copy: { Boolean: true } },
})

const looks = (style) => theme([key('accent', '#2563eb'), kind('checkbox', [key('style', style)])])

const flag = (name, type = 'bool') => ({
  name, node_type: type, children: [], is_hierarchy_transparent: false,
  parameters: { value: { Boolean: false } },
})

const div = (name, children) => ({
  name, node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children,
})

const boxAt = (path) => document.querySelector(`[data-path='${JSON.stringify(path)}'] input[type="checkbox"]`)
const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('a checkbox in the theme', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
  })

  it('is a switch where its container says so, and a box outside it and where an inner one says box', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        flag('outside'),
        div('side', [
          looks('switch'),
          flag('inside'),
          flag('ticked', 'checkbox'),
          div('back', [looks('box'), flag('again')]),
        ]),
      ] },
      looks('box'),
    ]
    app.renderer.renderDocument(app.currentDocument)
    await settled()

    expect(document.documentElement.style.getPropertyValue('--ov-checkbox-style')).toBe('box')
    expect(document.querySelector("[data-path='[\"t\",\"side\"]']").style.getPropertyValue('--ov-checkbox-style')).toBe('switch')
    expect(boxAt(['t', 'outside']).classList.contains('ov-switch')).toBe(false)
    expect(boxAt(['t', 'side', 'inside']).classList.contains('ov-switch')).toBe(true)
    expect(boxAt(['t', 'side', 'ticked']).classList.contains('ov-switch')).toBe(true)
    expect(boxAt(['t', 'side', 'back', 'again']).classList.contains('ov-switch')).toBe(false)
  })

  it('is a switch across the whole page when the document top says so', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        div('a', [flag('one')]),
      ] },
      looks('switch'),
    ]
    app.renderer.renderDocument(app.currentDocument)
    await settled()
    expect(boxAt(['t', 'a', 'one']).classList.contains('ov-switch')).toBe(true)
  })

  it('is still the same input, so pressing a switch changes its field', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: { mutable: { Boolean: true } }, is_hierarchy_transparent: true, children: [
        flag('done'),
      ] },
      looks('switch'),
    ]
    app.renderer.renderDocument(app.currentDocument)
    await settled()
    const box = boxAt(['t', 'done'])
    expect(box.classList.contains('ov-switch')).toBe(true)
    box.checked = true
    box.dispatchEvent(new Event('change'))
    const value = app.currentDocument[0].children[0].parameters.value
    expect(value === true || value?.Boolean === true).toBe(true)
  })
})
