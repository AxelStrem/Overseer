import { describe, it, expect, beforeEach, vi } from 'vitest'

// A vocabulary entry with an `icon` field shows that library icon before its name, in every chip
// the vocabulary draws - an enum, a tags field, a `<#key>` in text - so a person's chosen icon,
// a stage's or a label's are drawn the same way. An entry without one is drawn as before, and an
// icon field that is no library name draws nothing.

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
    parameters: { mutable: { Boolean: true } },
    children: [
      Object.assign(base('People', 'list'), {
        parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
        children: [
          person('claude', '**Claude**', 'bot'),
          person('bob', 'Bob', ''),
          person('mallory', 'Mallory', '../../x'),
        ],
      }),
      valued('owner', 'enum', owner, { vocabulary: PEOPLE }),
      valued('with', 'tags', 'claude, bob', { vocabulary: PEOPLE }),
      valued('note', 'text', 'asked <#claude>', { markdown: { Boolean: true }, vocabulary: PEOPLE }),
    ],
  }),
])

const chipsIn = (selector) => Array.from(document.querySelectorAll(`${selector} .tag-chip`))

describe('a vocabulary icon', () => {
  const render = (d) => {
    const app = new OverseerApp()
    app.currentDocument = d
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('comes before the name in an enum chip', () => {
    render(doc('claude'))
    const [chip] = chipsIn('.enum-field')
    const icon = chip.firstElementChild
    expect(icon.classList.contains('chip-icon')).toBe(true)
    expect(icon.dataset.icon).toBe('bot')
    expect(chip.querySelector('strong').textContent).toBe('Claude')
  })

  it('is drawn in a tags chip and in a chip in text, only for an entry that has one', () => {
    render(doc('bob'))
    const tags = chipsIn('.tags-field').filter((c) => !c.classList.contains('enum-choice'))
    expect(tags.map((c) => c.querySelector('.chip-icon')?.dataset.icon ?? null)).toEqual(['bot', null])
    const inline = document.querySelector('.inline-chip')
    expect(inline.querySelector('.chip-icon').dataset.icon).toBe('bot')
    const [owner] = chipsIn('.enum-field')
    expect(owner.querySelector('.chip-icon')).toBeNull()
    expect(owner.textContent).toBe('Bob')
  })

  it('draws nothing for a name the library cannot have', () => {
    render(doc('mallory'))
    const [chip] = chipsIn('.enum-field')
    expect(chip.querySelector('.chip-icon')).toBeNull()
    expect(chip.textContent).toBe('Mallory')
  })
})
