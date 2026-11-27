import { describe, it, expect, beforeEach, vi } from 'vitest'

// A tracker names its features with `<$HANDLE>`: the template's vocabulary reads `$/project/Features`
// for that mark and leaves the items to `#`, and a feature's hidden `shape` gives its chip its own form.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, value, more = {}) => Object.assign(base(name, type), {
  parameters: { value: { String: value }, ...more },
})

const entry = (template, handle, colour, shape) => Object.assign(base(`${template}__${handle}`, 'div'), {
  children: [valued('handle', 'string', handle), valued('colour', 'string', colour),
    valued('shape', 'string', shape, { hidden: { Boolean: true } })],
})

const keyed = (name, entries) => Object.assign(base(name, 'list'), {
  parameters: { key: { String: 'handle' }, hidden: { Boolean: true } },
  children: entries,
})

const aVocabulary = (elements) => Object.assign(base('vocabulary', 'context'), {
  parameters: { _context_copy: { Boolean: true } },
  children: Object.entries(elements).map(([name, said]) => valued(name, 'string', said)),
})

const tracker = (note) => Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    aVocabulary({ items: '/project/Items', features: '$/project/Features' }),
    keyed('Items', [entry('Item', 'SEARCH', '#1d4ed8', 'ticket')]),
    keyed('Features', [entry('Feature', 'SEARCH', '#be123c', 'hex')]),
    valued('note', 'text', note, { markdown: { Boolean: true } }),
  ],
})

const chips = () => Array.from(document.querySelectorAll('.text-field .tag-chip'))
  .map(c => [c.dataset.mark, c.textContent, c.style.backgroundColor, c.classList.contains('chip-shape-hex'),
    c.classList.contains('chip-shape-ticket')])

describe('a feature named in text', () => {
  beforeEach(() => setupDOM())

  it('is a chip in the shape its feature gives, under its own mark', () => {
    const app = new OverseerApp()
    app.currentDocument = [tracker('<$SEARCH>')]
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
    expect(chips()).toEqual([['$', 'SEARCH', 'rgb(190, 18, 60)', true, false]])
  })

  it('is not found by the mark of the items, which read only the items', () => {
    const app = new OverseerApp()
    app.currentDocument = [tracker('<#SEARCH>')]
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
    expect(chips()).toEqual([['#', 'SEARCH', 'rgb(29, 78, 216)', false, true]])
  })
})
