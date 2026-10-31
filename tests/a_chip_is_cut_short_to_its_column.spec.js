import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A chip is never wider than the place it is drawn in: a table's column is a fixed share of the
// width, and a chip that kept its own ran over the column beside it. Its name is held in one
// span, which the stylesheet cuts short with an ellipsis, while its icon and cross keep their
// size; and a name cut short says itself in full on the pointer.

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

const person = (tag, name, icon) => Object.assign(base(`Person__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', tag),
    valued('name', 'string', name),
    valued('icon', 'string', icon),
  ],
})

const PEOPLE = { String: '/people/People' }

const doc = (owner) => ([
  Object.assign(base('people', 'tab'), {
    parameters: { mutable: { Boolean: false } },
    children: [
      Object.assign(base('People', 'list'), {
        parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
        children: [
          person('ap31', 'Dementii Petrovich', 'crown'),
          person('bob', 'Bob', ''),
        ],
      }),
      valued('owner', 'enum', owner, { vocabulary: PEOPLE }),
    ],
  }),
])

const ownerChip = () => document.querySelector('.enum-field .tag-chip')

// jsdom lays nothing out, so the widths a browser would measure are given.
const measured = (el, scrollWidth, clientWidth) => {
  Object.defineProperty(el, 'scrollWidth', { value: scrollWidth, configurable: true })
  Object.defineProperty(el, 'clientWidth', { value: clientWidth, configurable: true })
}

describe('a chip in a narrow place', () => {
  const render = (d) => {
    const app = new OverseerApp()
    app.currentDocument = d
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('holds its name in one span, after its icon', () => {
    render(doc('ap31'))
    const chip = ownerChip()
    expect(chip.children[0].classList.contains('chip-icon')).toBe(true)
    expect(chip.children[1].classList.contains('chip-name')).toBe(true)
    expect(chip.children[1].textContent).toBe('Dementii Petrovich')
    expect(chip.textContent).toBe('Dementii Petrovich')
  })

  it('says its name in full on the pointer only when it is cut short', () => {
    render(doc('ap31'))
    const chip = ownerChip()
    measured(chip.querySelector('.chip-name'), 120, 40)
    chip.dispatchEvent(new Event('mouseenter'))
    expect(chip.title).toBe('Dementii Petrovich')

    render(doc('bob'))
    const whole = ownerChip()
    measured(whole.querySelector('.chip-name'), 30, 30)
    whole.dispatchEvent(new Event('mouseenter'))
    expect(whole.title).toBe('')
  })

  it('is held to its place by the stylesheet, its name giving way first', () => {
    const css = readFileSync('src/styles.css', 'utf8')
    expect(css).toMatch(/\.tag-chip \{\s*max-width: 100%;\s*min-width: 0;/)
    expect(css).toMatch(/\.tag-chip \.chip-name \{[^}]*text-overflow: ellipsis;/)
    expect(css).toMatch(/\.tag-chip > \.chip-icon,\s*\.tag-chip > \.tag-remove \{\s*flex-shrink: 0;/)
  })
})
