import { describe, it, expect, beforeEach, vi } from 'vitest'

// The page draws through the `theme` context. The copy the resolver works out at the document's top
// is the user's theme, written as `--ov-<key>` variables on the page's root; a container that
// changes it holds a copy of its own, written on its element, so the browser hands it down to
// what is inside and nothing else. The `mode` beside it is written as `data-ov-mode`.

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

const key = (name, value, computed) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: Object.assign(
    { value: computed ? { Formula: 'x' } : { String: value } },
    computed ? { _computed_value: { String: value } } : {},
  ),
})

const theme = (keys, copy = true) => ({
  name: 'theme', node_type: 'context', is_hierarchy_transparent: false, children: keys,
  parameters: copy ? { _context_copy: { Boolean: true } } : {},
})

const mode = (value) => ({
  name: 'mode', node_type: 'context', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value }, _context_copy: { Boolean: true } },
})

const field = (name, value) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value } },
})

const elementAt = (path) => document.querySelector(`[data-path='${JSON.stringify(path)}']`)

describe('the theme', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
    delete document.documentElement.dataset.ovMode
  })

  it('at the document top is the whole page\'s, its formulas as worked out', () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [field('shown', 'x')] },
      theme([key('accent', '#2563eb'), key('surface', '#1e293b', true)]),
      mode('dark'),
    ]
    app.renderer.renderDocument(app.currentDocument)
    const root = document.documentElement
    expect(root.style.getPropertyValue('--ov-accent')).toBe('#2563eb')
    expect(root.style.getPropertyValue('--ov-surface')).toBe('#1e293b')
    expect(root.dataset.ovMode).toBe('dark')
  })

  it('writes the colours a mode keeps under light and dark only through the keys that pick them', () => {
    const app = new OverseerApp()
    const held = (name, keys) => ({ name, node_type: '-', children: keys, is_hierarchy_transparent: false, parameters: {} })
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [field('shown', 'x')] },
      theme([
        held('light', [key('background', '#f8fafc')]),
        held('dark', [key('background', '#000000')]),
        key('background', '#000000', true),
      ]),
      mode('dark'),
    ]
    app.renderer.renderDocument(app.currentDocument)
    const root = document.documentElement
    expect(root.style.getPropertyValue('--ov-background')).toBe('#000000')
    expect(root.style.getPropertyValue('--ov-light-background')).toBe('')
    expect(root.style.getPropertyValue('--ov-dark-background')).toBe('')
  })

  it('a container changes for what is inside it, and leaves its neighbours to the page\'s', () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        { name: 'side', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
          theme([key('accent', '#2563eb'), key('surface', '#16203a')]),
          mode('dark'),
          theme([key('surface', '#16203a')], false),
          field('inside', 'in'),
        ] },
        { name: 'main', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
          field('outside', 'out'),
        ] },
      ] },
      theme([key('accent', '#2563eb'), key('surface', '#ffffff')]),
    ]
    app.renderer.renderDocument(app.currentDocument)
    const side = elementAt(['t', 'side'])
    const main = elementAt(['t', 'main'])
    expect(side.style.getPropertyValue('--ov-surface')).toBe('#16203a')
    expect(side.dataset.ovMode).toBe('dark')
    expect(main.style.getPropertyValue('--ov-surface')).toBe('')
    expect(document.documentElement.style.getPropertyValue('--ov-surface')).toBe('#ffffff')
  })

  it('drawn again in another, leaves nothing of the last behind', () => {
    const app = new OverseerApp()
    const body = { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [field('shown', 'x')] }
    app.renderer.renderDocument([body, theme([key('accent', 'red'), key('line', 'grey')]), mode('dark')])
    app.renderer.renderDocument([body, theme([key('accent', 'blue')])])
    const root = document.documentElement
    expect(root.style.getPropertyValue('--ov-accent')).toBe('blue')
    expect(root.style.getPropertyValue('--ov-line')).toBe('')
    expect(root.dataset.ovMode).toBeUndefined()
  })
})
