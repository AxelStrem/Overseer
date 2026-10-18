import { describe, it, expect, beforeEach, vi } from 'vitest'

// `<#key>` in any text the page draws is the chip of the entry the nearest vocabulary knows by
// that key. A `context vocabulary` said on a container is seen by everything inside it, so the text
// of a comment and a field naming the items it waits on draw from the one list, and pressing a
// chip that stands for an entry shows that entry.

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

const item = (handle, title, colour, fields = []) => Object.assign(base(`Item__${handle}`, 'div'), {
  children: [
    valued('handle', 'string', handle),
    valued('title', 'string', title),
    valued('colour', 'string', colour),
    ...fields,
  ],
})

const ITEMS = { String: '/project/Items' }

/// What the resolver puts first in a container that declares `context vocabulary = ...`.
const aVocabulary = (value) => Object.assign(base('vocabulary', 'context'), {
  parameters: { value, _context_copy: { Boolean: true } },
})

/// Two tabs: a page of notes, and the project with its items. The notes declare the vocabulary on
/// their tab; the project on its own.
const docWith = ({ note = '', markdown = true, after = null, parentOf = false } = {}) => ([
  Object.assign(base('notes', 'tab'), {
    parameters: { mutable: { Boolean: true } },
    children: [
      aVocabulary(ITEMS),
      valued('note', 'text', note, { markdown: { Boolean: markdown } }),
      ...(after === null ? [] : [valued('after', 'tags', after)]),
      valued('labels', 'tags', 'ui', { vocabulary: { String: '/project/Labels' } }),
    ],
  }),
  Object.assign(base('project', 'tab'), {
    parameters: { mutable: { Boolean: true } },
    children: [
      aVocabulary(ITEMS),
      Object.assign(base('Labels', 'list'), {
        parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
        children: [Object.assign(base('Label__ui', 'div'), {
          children: [valued('tag', 'string', 'ui'), valued('colour', 'string', '#0f766e')],
        })],
      }),
      Object.assign(base('Items', 'list'), {
        parameters: { key: { String: 'handle' } },
        children: ['parser', 'escape'].map((h, i) => item(h, `the ${h}`, i ? '#a16207' : '#1d4ed8',
          parentOf ? [valued('parent', 'enum', '')] : [])),
      }),
    ],
  }),
])

const press = (el) => el.dispatchEvent(new MouseEvent('click', { bubbles: true }))
const noteChips = () => Array.from(document.querySelectorAll('.text-field .tag-chip'))

describe('a key in text', () => {
  let app

  const render = (doc) => {
    app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('is drawn as the chip of the entry the vocabulary handed down knows by it', () => {
    render(docWith({ note: 'Waits on <#parser>, as **before**.' }))
    const [chip] = noteChips()
    expect(chip.textContent).toBe('parser')
    expect(chip.style.backgroundColor).toBe('rgb(29, 78, 216)')
    expect(chip.title).toBe('the parser')
    expect(chip.classList.contains('chip-pill') || !chip.closest('.chip-box')).toBe(true)
    expect(document.querySelector('.text-field strong').textContent).toBe('before')
  })

  it('is a chip in plain text as well as in markdown', () => {
    render(docWith({ note: 'see <#escape>', markdown: false }))
    expect(noteChips().map(c => c.textContent)).toEqual(['escape'])
    expect(noteChips()[0].style.backgroundColor).toBe('rgb(161, 98, 7)')
  })

  it('stays text inside markdown code, where the marker is written about', () => {
    render(docWith({ note: 'write `<#parser>` for a chip' }))
    expect(noteChips()).toEqual([])
    expect(document.querySelector('.text-field code').textContent).toBe('<#parser>')
  })

  it('names a key no vocabulary holds as a plain chip, not as a stray', () => {
    render(docWith({ note: 'and <#nowhere>' }))
    const [chip] = noteChips()
    expect(chip.textContent).toBe('nowhere')
    expect(chip.classList.contains('tag-unknown')).toBe(false)
    expect(chip.classList.contains('chip-reveals')).toBe(false)
  })

  it('gives a tags field naming no list the one handed down, and leaves one naming its own alone', () => {
    render(docWith({ after: 'escape' }))
    const fields = Array.from(document.querySelectorAll('.tags-field'))
    const [after, labels] = fields.map(f => f.querySelector('.tag-chip'))
    expect(after.dataset.tag).toBe('escape')
    expect(after.classList.contains('tag-unknown')).toBe(false)
    expect(after.style.backgroundColor).toBe('rgb(161, 98, 7)')
    expect(labels.style.backgroundColor).toBe('rgb(15, 118, 110)')
  })

  it('shows its entry when pressed, opening the tab it is on', () => {
    render(docWith({ note: 'see <#escape>' }))
    const pages = Array.from(document.querySelectorAll('.tab-content'))
    const project = pages.find(p => p.dataset.tabId === 'project')
    expect(project.style.display).toBe('none')
    press(noteChips()[0])
    expect(project.style.display).toBe('')
    const shown = project.querySelector('.revealed')
    expect(shown).not.toBeNull()
    expect(shown.textContent).toContain('the escape')
  })

  it('says why on hover, and shows nothing, when the entry is in a list the page does not draw', () => {
    const doc = docWith({ note: 'see <#escape>' })
    doc[1].children.find(c => c.name === 'Items').parameters.hidden = { Boolean: true }
    render(doc)
    const [chip] = noteChips()
    press(chip)
    expect(chip.title).toBe('the escape - not shown on this page')
    expect(document.querySelector('.revealed')).toBeNull()
  })

  it('offers no entry as its own parent', () => {
    render(docWith({ parentOf: true }))
    const parents = Array.from(document.querySelectorAll('.enum-field .tag-chip'))
    expect(parents.length).toBe(2)
    press(parents[0])
    const offered = Array.from(document.querySelectorAll('.tag-picker .tag-option')).map(o => o.textContent.trim())
    expect(offered).toEqual(['escape'])
  })
})
