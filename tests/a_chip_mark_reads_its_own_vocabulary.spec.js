import { describe, it, expect, beforeEach, vi } from 'vitest'

// Each mark of a chip in text - `<#key>`, `<@key>` and the rest - reads its own lists: those a
// vocabulary names with that mark in front of their paths, a path with none answering `#`. Each
// mark is looked for on its own, so a container naming only where its people are keeps the items
// above it; and a `context vocabulary` can be a block whose elements each name lists, merged as
// any context's are, which the page gathers from every copy up to the top.

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

const person = (tag, name, colour) => Object.assign(base(`Person__${tag}`, 'div'), {
  children: [valued('tag', 'string', tag), valued('name', 'string', name), valued('colour', 'string', colour)],
})

const item = (handle, colour) => Object.assign(base(`Item__${handle}`, 'div'), {
  children: [valued('handle', 'string', handle), valued('colour', 'string', colour)],
})

const keyed = (name, key, entries) => Object.assign(base(name, 'list'), {
  parameters: { key: { String: key }, hidden: { Boolean: true } },
  children: entries,
})

/// What the resolver puts first in a container that declares `context vocabulary`: its value, or
/// the elements of its block, or both.
const aVocabulary = (value, elements = {}) => Object.assign(base('vocabulary', 'context'), {
  parameters: { ...(value === null ? {} : { value: { String: value } }), _context_copy: { Boolean: true } },
  children: Object.entries(elements).map(([name, said]) => valued(name, 'string', said)),
})

const lists = () => Object.assign(base('p', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    keyed('Items', 'handle', [item('ap31', '#1d4ed8')]),
    keyed('People', 'tag', [person('ap31', 'Alice', '#be123c')]),
    keyed('Guests', 'tag', [person('ap31', 'Guest Alice', '#15803d')]),
  ],
})

const notes = (outer, inner, note) => Object.assign(base('notes', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    outer,
    Object.assign(base('side', 'div'), {
      children: [...(inner ? [inner] : []), valued('note', 'text', note, { markdown: { Boolean: true } })],
    }),
  ],
})

const chips = () => Array.from(document.querySelectorAll('.text-field .tag-chip'))
  .map(c => [c.dataset.mark, c.textContent, c.style.backgroundColor])

describe('a mark of a chip in text', () => {
  let app

  const render = (doc) => {
    app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('reads the lists its mark is written in front of, and an unmarked path answers #', () => {
    render([notes(aVocabulary('/p/Items, @/p/People'), null, '<#ap31> by <@ap31>'), lists()])
    expect(chips()).toEqual([
      ['#', 'ap31', 'rgb(29, 78, 216)'],
      ['@', 'Alice', 'rgb(190, 18, 60)'],
    ])
  })

  it('is looked for on its own, so a nearer vocabulary naming one mark keeps the others above', () => {
    render([notes(aVocabulary('/p/Items, @/p/People'), aVocabulary('@/p/Guests'), '<#ap31> by <@ap31>'), lists()])
    expect(chips()).toEqual([
      ['#', 'ap31', 'rgb(29, 78, 216)'],
      ['@', 'Guest Alice', 'rgb(21, 128, 61)'],
    ])
  })

  it('reads the elements of a vocabulary block, a nearer copy\'s element in place of the one above', () => {
    const outer = aVocabulary(null, { items: '/p/Items', people: '@/p/People' })
    render([notes(outer, aVocabulary(null, { people: '@/p/Guests' }), '<#ap31> by <@ap31>'), lists()])
    expect(chips()).toEqual([
      ['#', 'ap31', 'rgb(29, 78, 216)'],
      ['@', 'Guest Alice', 'rgb(21, 128, 61)'],
    ])
  })

  it('reads a value and the elements added to it together', () => {
    render([notes(aVocabulary('/p/Items', { people: '@/p/People' }), null, '<#ap31> by <@ap31>'), lists()])
    expect(chips().map(c => c[1])).toEqual(['ap31', 'Alice'])
  })

  it('names nothing in a vocabulary that has no list for it', () => {
    render([notes(aVocabulary('/p/Items'), null, 'by <@ap31> and <%ap31>'), lists()])
    expect(chips()).toEqual([['@', 'ap31', ''], ['%', 'ap31', '']])
  })

  it('is any of the marks markdown leaves as text, and stays text inside code', () => {
    render([notes(aVocabulary('/p/Items'), null, '<@a> <$b> <:c> <%d> <~e> <^f> <+g> `<@h>`'), lists()])
    expect(chips().map(c => c[0] + c[1])).toEqual(['@a', '$b', ':c', '%d', '~e', '^f', '+g'])
    expect(document.querySelector('.text-field code').textContent).toBe('<@h>')
  })

  it('draws in plain text as well as in markdown', () => {
    const doc = [notes(aVocabulary('@/p/People'), null, 'by <@ap31>'), lists()]
    doc[0].children[1].children[0].parameters.markdown = { Boolean: false }
    render(doc)
    expect(chips().map(c => c[1])).toEqual(['Alice'])
  })
})
