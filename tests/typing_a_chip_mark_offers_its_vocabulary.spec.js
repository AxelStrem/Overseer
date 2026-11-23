import { describe, it, expect, beforeEach, vi } from 'vitest'

// A chip mark typed in the text editor - `#ma` - opens a drop-down of the entries the vocabulary
// for that mark holds that answer what follows it, and picking one writes the chip in place of
// what was typed: `<#markdown-editor>`. Only a mark with lists to read opens it, and only once a
// letter follows the mark, so a heading and a new line are as they were.

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
import { chipMarkAtCaret, vocabularyMatches } from '../src/renderer.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, value, more = {}) => Object.assign(base(name, type), {
  parameters: { value: { String: value }, ...more },
})

const item = (handle, title) => Object.assign(base(`Item__${handle}`, 'div'), {
  children: [valued('handle', 'string', handle), valued('title', 'string', title)],
})

const person = (tag, name) => Object.assign(base(`Person__${tag}`, 'div'), {
  children: [valued('tag', 'string', tag), valued('name', 'string', name)],
})

const keyed = (name, key, entries) => Object.assign(base(name, 'list'), {
  parameters: { key: { String: key }, hidden: { Boolean: true } },
  children: entries,
})

const aVocabulary = (value) => Object.assign(base('vocabulary', 'context'), {
  parameters: { value: { String: value }, _context_copy: { Boolean: true } },
})

const aDocument = () => [Object.assign(base('p', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    aVocabulary('/p/Items, @/p/People'),
    keyed('Items', 'handle', [
      item('parser', 'The parser reads nested lists'),
      item('markdown-editor', 'Markdown Editor for text nodes'),
      item('mark-complete', 'Typing a chip mark offers its vocabulary'),
    ]),
    keyed('People', 'tag', [person('ap31', 'Alice')]),
    valued('note', 'text', '', { markdown: { Boolean: true } }),
  ],
})]

describe('typing a chip mark in the text editor', () => {
  let textarea

  beforeEach(() => {
    setupDOM()
    const app = new OverseerApp()
    window.app = app
    app.currentDocument = aDocument()
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
    document.querySelector('.text-content').dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    textarea = document.querySelector('textarea.markdown-editor')
  })

  const type = (text) => {
    textarea.value = text
    textarea.setSelectionRange(text.length, text.length)
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
  }
  const key = (name) => {
    const e = new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true })
    textarea.dispatchEvent(e)
    return e
  }
  const offered = () => Array.from(document.querySelectorAll('.mark-complete .mark-option')).map(o => o.dataset.key)

  it('offers the entries that answer what follows the mark, keys beginning with it first', () => {
    type('see #ma')
    expect(offered()).toEqual(['markdown-editor', 'mark-complete'])
    type('see #nested')
    expect(offered()).toEqual(['parser'])
  })

  it('writes the chip picked with Enter in place of what was typed', () => {
    type('see #ma')
    expect(key('Enter').defaultPrevented).toBe(true)
    expect(textarea.value).toBe('see <#markdown-editor>')
    expect(textarea.selectionStart).toBe(textarea.value.length)
    expect(offered()).toEqual([])
  })

  it('moves with the arrows and picks with a click', () => {
    type('#ma and more')
    textarea.setSelectionRange(3, 3)
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    key('ArrowDown')
    expect(document.querySelector('.mark-option-lit').dataset.key).toBe('mark-complete')
    type('(#mark')
    document.querySelectorAll('.mark-option')[1].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
    expect(textarea.value).toBe('(<#mark-complete>')
  })

  it('reads the lists of the mark typed', () => {
    type('for @al')
    expect(offered()).toEqual(['ap31'])
    key('Tab')
    expect(textarea.value).toBe('for <@ap31>')
  })

  it('offers nothing for a mark with no lists, a heading, a lone mark or a chip already written', () => {
    for (const text of ['cost $al', '# heading', '#', 'a@', '<#ma', 'see #ma ']) {
      type(text)
      expect(offered(), text).toEqual([])
    }
  })

  it('closes on Escape and leaves the editor open', () => {
    type('see #ma')
    key('Escape')
    expect(offered()).toEqual([])
    expect(document.querySelector('textarea.markdown-editor')).toBe(textarea)
    key('Escape')
    expect(document.querySelector('textarea.markdown-editor')).toBeNull()
  })

  it('lets Ctrl+Enter save what was typed as it stands', () => {
    type('see #ma')
    const e = new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true, cancelable: true })
    textarea.dispatchEvent(e)
    expect(e.defaultPrevented).toBe(false)
    expect(document.querySelector('.mark-complete')).toBeNull()
    expect(document.querySelector('.text-content').textContent.trim()).toBe('see #ma')
  })

  it('closes when the editor does', () => {
    type('see #ma')
    document.querySelector('.markdown-editor-container .cancel-btn').click()
    expect(document.querySelector('.mark-complete')).toBeNull()
  })
})

describe('the mark at the caret', () => {
  it('is the whole word the caret is in', () => {
    expect(chipMarkAtCaret('a #mark-x b', 5)).toEqual({ mark: '#', query: 'ma', start: 2, end: 9 })
  })

  it('matches names and hover text after keys', () => {
    const v = new Map([['b', { name: 'Zed', hover: 'about apples' }], ['apple', { name: 'apple' }], ['c', { name: 'Apple pie', offered: false }]])
    expect(vocabularyMatches(v, 'ap')).toEqual(['apple', 'b'])
  })
})
