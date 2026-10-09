import { describe, it, expect, beforeEach, vi } from 'vitest'

// A filter can narrow by more than one field holding tags - a project item's own tags and its
// workflow flags, say - with a row of chips for each. The first row comes from the filter's own
// vocabulary, the others from their fields', and a chip asks about its own field only: a flag
// picked never matches a project tag that happens to share its name.

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

const valued = (name, type, variant, value) => Object.assign(base(name, type), {
  parameters: { value: { [variant]: value } },
})

const entryOf = (list, tag, colour) => Object.assign(base(`${list}__${tag}`, 'div'), {
  children: [valued('tag', 'string', 'String', tag),
             valued('name', 'string', 'String', tag),
             valued('colour', 'string', 'String', colour)],
})

const tagsField = (name, value, vocabulary) => Object.assign(valued(name, 'tags', 'String', value), {
  parameters: { value: { String: value }, vocabulary: { String: vocabulary } },
})

const item = (n, title, labels, flags) => Object.assign(base(`Item__${n}`, 'div'), {
  children: [
    valued('title', 'string', 'String', title),
    tagsField('labels', labels, '/project/Labels'),
    tagsField('flags', flags, '/project/Flags'),
  ],
})

const docOf = () => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('Labels', 'list'), {
      parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
      // A project tag named like a flag, to show the two are never confused.
      children: [entryOf('Label', 'bug', '#d73a4a'), entryOf('Label', 'ui', '#1d76db'),
                 entryOf('Label', 'manual', '#0e8a16')],
    }),
    Object.assign(base('Flags', 'list'), {
      parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
      children: [entryOf('Flag', 'manual', '#78716c'), entryOf('Flag', 'first', '#e11d48'),
                 entryOf('Flag', 'asked', '#b45309')],
    }),
    Object.assign(base('filter', 'filter'), {
      parameters: {
        target: { String: '/project/Items' },
        text: { String: 'title' },
        tags: { String: 'labels, flags' },
        vocabulary: { String: '/project/Labels' },
      },
    }),
    Object.assign(base('Items', 'list'), {
      children: [
        item(1, 'Parser drops a brace', 'bug', 'first'),
        item(2, 'Write the guide', 'manual', ''),
        item(3, 'Tighten the rows', 'ui', 'manual'),
        item(4, 'Themes', 'ui', 'first, asked'),
        item(5, 'Something new', 'bug', ''),
      ],
    }),
  ],
})])

const visible = () => Array.from(document.querySelectorAll('[data-path]'))
  .filter((element) => {
    let path
    try { path = JSON.parse(element.dataset.path) } catch { return false }
    if (path.length !== 3 || path[1] !== 'Items') return false
    return !element.classList.contains('filtered-out')
  })
  .map((element) => {
    const path = JSON.parse(element.dataset.path)
    const title = document.querySelector(`[data-path='${JSON.stringify([...path, 'title'])}']`)
    return (title?.textContent || '').trim()
  })

const chipsOf = (field) => Array.from(document.querySelectorAll(`.filter-tags[data-filter-field='${field}'] .filter-tag`))
const pick = (field, tag) => chipsOf(field).find((chip) => chip.dataset.tag === tag)
  .dispatchEvent(new Event('click', { bubbles: true }))

describe('a filter over tags and flags', () => {
  let app

  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
    app.currentDocument = docOf()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('offers a row of chips for each field, each from its own list', () => {
    expect(chipsOf('labels').map((c) => c.dataset.tag)).toEqual(['bug', 'ui', 'manual'])
    expect(chipsOf('flags').map((c) => c.dataset.tag)).toEqual(['manual', 'first', 'asked'])
    expect(chipsOf('flags')[1].style.backgroundColor).toBe('rgb(225, 29, 72)')
  })

  it('narrows by a flag, asking only the flags field', () => {
    pick('flags', 'manual')
    expect(visible()).toEqual(['Tighten the rows'])
  })

  it('narrows by a tag, asking only the tags field', () => {
    pick('labels', 'manual')
    expect(visible()).toEqual(['Write the guide'])
  })

  it('a tag and a flag together find what holds both', () => {
    pick('labels', 'bug')
    pick('flags', 'first')
    expect(visible()).toEqual(['Parser drops a brace'])
  })

  it('two flags find what holds both', () => {
    pick('flags', 'first')
    pick('flags', 'asked')
    expect(visible()).toEqual(['Themes'])
  })

  it('counts what is showing, and lets a flag go again', () => {
    pick('flags', 'first')
    expect(document.querySelector('.filter-count').textContent).toBe('2 of 5')
    pick('flags', 'first')
    expect(visible().length).toBe(5)
    expect(document.querySelector('.filter-count').textContent).toBe('5')
  })
})
