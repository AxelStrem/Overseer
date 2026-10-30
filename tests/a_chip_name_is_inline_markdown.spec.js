import { describe, it, expect, beforeEach, vi } from 'vitest'

// The name a vocabulary entry gives is inline markdown, drawn wherever its chip is - in a tags
// field, an enum, a `<#key>` in text and a picker - so a person's name can be coloured letter by
// letter or carry an icon. Raw HTML stays text as it does in a text field, a link is its text,
// and an entry that gives no name is drawn by its key as it stands.

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
import { nameAsText } from '../src/renderer.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, value, more = {}) => Object.assign(base(name, type), {
  parameters: { value: { String: value }, ...more },
})

const STYLED = '**C**<color=#e11d48 | la>ude <icon=bot>'

const person = (tag, name) => Object.assign(base(`Person__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', tag),
    ...(name === null ? [] : [valued('name', 'string', name)]),
    valued('colour', 'string', '#1d4ed8'),
  ],
})

const PEOPLE = { String: '/people/People' }

const doc = ({ owner = 'claude', note = '', with_ = '' } = {}) => ([
  Object.assign(base('people', 'tab'), {
    parameters: { mutable: { Boolean: true } },
    children: [
      Object.assign(base('People', 'list'), {
        parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
        children: [
          person('claude', STYLED),
          person('mallory', '<img src=x onerror="window.ran=1"> [away](https://example.com)'),
          person('__init__', null),
          person('bot', '<icon=bot>'),
        ],
      }),
      valued('owner', 'enum', owner, { vocabulary: PEOPLE }),
      valued('with', 'tags', with_, { vocabulary: PEOPLE }),
      valued('note', 'text', note, { markdown: { Boolean: true }, vocabulary: PEOPLE }),
    ],
  }),
])

const chipsIn = (selector) => Array.from(document.querySelectorAll(`${selector} .tag-chip`))

describe('a chip name', () => {
  let app

  const render = (d) => {
    app = new OverseerApp()
    app.currentDocument = d
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('is drawn as markdown in an enum, coloured letters and icon included, as one piece', () => {
    render(doc())
    const [chip] = chipsIn('.enum-field')
    const name = chip.querySelector('.chip-name')
    expect(name).not.toBeNull()
    expect(name.querySelector('strong').textContent).toBe('C')
    const coloured = name.querySelector('.md-inline-color')
    expect(coloured.textContent).toBe('la')
    expect(coloured.style.color).toBe('rgb(225, 29, 72)')
    expect(name.querySelector('.icon[data-icon]')).not.toBeNull()
    expect(chip.textContent.trim()).toBe('Claude')
    expect(chip.dataset.text).toBe(STYLED)
    expect(chip.style.backgroundColor).toBe('rgb(29, 78, 216)')
  })

  it('is drawn the same in a tags field and in a key in text', () => {
    render(doc({ with_: 'claude', note: 'asked by <#claude>' }))
    const [tag] = chipsIn('.tags-field')
    expect(tag.querySelector('.chip-name strong').textContent).toBe('C')
    const [inText] = chipsIn('.text-field')
    expect(inText.querySelector('.chip-name strong').textContent).toBe('C')
    expect(inText.querySelector('.md-inline-color').textContent).toBe('la')
  })

  it('keeps raw HTML as text and draws a link as its words', () => {
    window.ran = 0
    render(doc({ owner: 'mallory' }))
    const [chip] = chipsIn('.enum-field')
    expect(chip.querySelector('img')).toBeNull()
    expect(chip.querySelector('a')).toBeNull()
    expect(chip.textContent).toContain('<img src=x')
    expect(chip.textContent).toContain('away')
    expect(window.ran).toBe(0)
  })

  it('is the key as it stands when the entry gives no name', () => {
    render(doc({ owner: '__init__' }))
    const [chip] = chipsIn('.enum-field')
    expect(chip.querySelector('strong')).toBeNull()
    expect(chip.textContent).toBe('__init__')
  })

  it('says its key on hover when it shows only an icon', () => {
    render(doc({ owner: 'bot' }))
    expect(chipsIn('.enum-field')[0].title).toMatch(/^bot\b/)
    render(doc({ owner: 'claude' }))
    expect(chipsIn('.enum-field')[0].title).not.toMatch(/^claude\b/)
  })

  it('reads as plain text where only text can go', () => {
    expect(nameAsText(STYLED)).toBe('Claude')
    expect(nameAsText('*agent* ~~old~~ <#ap31>')).toBe('agent old ap31')
    expect(nameAsText('agent_ap31')).toBe('agent_ap31')
    expect(nameAsText('<icon=bot> Bot')).toBe('Bot')
  })
})
