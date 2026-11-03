import { describe, it, expect, beforeEach, vi } from 'vitest'

// A textbox that says `narrows=` hides the entries of that list it does not match, as it is typed
// into, matched against the fields `by` names. Given a vocabulary it picks values instead: an
// entry needs all of them in a tags field, and any of them in an enum field. What it holds is a
// view - never written, never sent with a press - and it can be typed into in a document that is
// read-only.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: name === '',
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})
const valued = (name, type, value, more = {}) => base(name, type, { value: { String: value }, ...more })
const said = (params) => Object.fromEntries(Object.entries(params).map(([k, v]) => [k, { String: v }]))

const tagOf = (list, tag) => base(`${list}__${tag}`, 'div', {}, [valued('tag', 'string', tag), valued('name', 'string', tag)])
const vocabularyList = (name, entry, tags) =>
  base(name, 'list', { key: { String: 'tag' }, hidden: { Boolean: true } }, tags.map((t) => tagOf(entry, t)))

// Each field inside a layout div, as every real document puts them, and a conversation to search.
const item = (n, title, stage, labels, flags, said = []) => base(`Item__${n}`, 'div', {}, [
  base('', 'div', {}, [
    valued('title', 'string', title),
    valued('stage', 'enum', stage, { vocabulary: { String: '/project/Stages' } }),
    valued('labels', 'tags', labels, { vocabulary: { String: '/project/Labels' } }),
    valued('flags', 'tags', flags, { vocabulary: { String: '/project/Flags' } }),
  ]),
  base('Comments', 'list', {}, said.map((body, i) => base(`Comment__${i + 1}`, 'div', {}, [valued('body', 'string', body)]))),
])

const box = (name, params) => valued(name, 'textbox', '', said(params))

const docOf = ({ mutable = true } = {}) => ([base('project', 'tab', { mutable: { Boolean: mutable } }, [
  vocabularyList('Stages', 'Stage', ['filed', 'ready', 'later']),
  vocabularyList('Labels', 'Label', ['bug', 'ui']),
  vocabularyList('Flags', 'Flag', ['first', 'manual']),
  base('', 'div', { layout: { String: 'horizontal' } }, [
    box('find', { placeholder: 'find', narrows: '/project/Items', by: 'title, Comments' }),
    box('stages', { vocabulary: '/project/Stages', narrows: '/project/Items', by: 'stage' }),
    box('labels', { vocabulary: '/project/Labels', narrows: '/project/Items', by: 'labels' }),
    box('flags', { vocabulary: '/project/Flags', narrows: '/project/Items', by: 'flags' }),
  ]),
  base('Items', 'list', {}, [
    item(1, 'Parser drops a brace', 'ready', 'bug', 'first'),
    item(2, 'Write the guide', 'filed', '', '', ['the parser is fine']),
    item(3, 'Tighten the rows', 'ready', 'bug, ui', ''),
    item(4, 'Themes', 'later', 'ui', 'manual'),
  ]),
])])

// By entry, not by element: the entry and the layout div inside it share one path.
const visible = () => {
  const shown = new Map()
  for (const element of document.querySelectorAll('[data-path]')) {
    let path
    try { path = JSON.parse(element.dataset.path) } catch { continue }
    if (path.length !== 3 || path[1] !== 'Items') continue
    shown.set(element.dataset.path, (shown.get(element.dataset.path) ?? true) && !element.classList.contains('filtered-out'))
  }
  return [...shown].filter(([, on]) => on).map(([key]) => {
    const title = document.querySelector(`[data-path='${JSON.stringify([...JSON.parse(key), 'title'])}']`)
    return (title?.textContent || '').trim()
  })
}

const fieldOf = (name) => document.querySelector(`[data-path='${JSON.stringify(['project', name])}']`)
const type = (what) => {
  const input = fieldOf('find').querySelector('input')
  input.value = what
  input.dispatchEvent(new Event('input', { bubbles: true }))
}
const pick = (name, value) => {
  fieldOf(name).querySelector('.tag-add').click()
  Array.from(document.querySelectorAll('.tag-option')).find((o) => o.dataset.tag === value).click()
}
let app
const open = (doc) => {
  setupDOM()
  app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
}

describe('a textbox that narrows a list', () => {
  beforeEach(() => open(docOf()))

  it('hides nothing while it is empty', () => {
    expect(visible()).toEqual(['Parser drops a brace', 'Write the guide', 'Tighten the rows', 'Themes'])
  })

  it('finds the typed text in any of its fields, a conversation included', () => {
    type('PARSER')
    expect(visible()).toEqual(['Parser drops a brace', 'Write the guide'])
    type('')
    expect(visible().length).toBe(4)
  })

  it('wants every picked tag, and any picked stage', () => {
    pick('labels', 'bug')
    pick('labels', 'ui')
    expect(visible()).toEqual(['Tighten the rows'])
    open(docOf())
    pick('stages', 'ready')
    pick('stages', 'later')
    expect(visible()).toEqual(['Parser drops a brace', 'Tighten the rows', 'Themes'])
  })

  it('shows only what every box lets through', () => {
    pick('stages', 'ready')
    pick('flags', 'first')
    expect(visible()).toEqual(['Parser drops a brace'])
    type('rows')
    expect(visible()).toEqual([])
  })

  it('asks a flag of the flags field, not of the labels', () => {
    pick('labels', 'ui')
    pick('flags', 'manual')
    expect(visible()).toEqual(['Themes'])
  })

  it('keeps what it holds through a repaint, and never sends it with a press', () => {
    type('theme')
    pick('labels', 'ui')
    app.renderer.renderDocument(app.currentDocument)
    app.renderer.applyFilters()
    expect(fieldOf('find').querySelector('input').value).toBe('theme')
    expect(visible()).toEqual(['Themes'])
    expect(app.renderer.typedText()).toEqual([])
  })

  it('is typed into and picked from in a document that cannot be changed', () => {
    open(docOf({ mutable: false }))
    expect(fieldOf('find').querySelector('input').readOnly).toBe(false)
    pick('labels', 'ui')
    expect(visible()).toEqual(['Tighten the rows', 'Themes'])
  })

  it('narrows beside the older filter node, both at once', () => {
    const doc = docOf()
    doc[0].children.push(base('old', 'filter', said({ target: '/project/Items', text: 'title' })))
    open(doc)
    pick('stages', 'ready')
    const old = document.querySelector('.filter-text')
    old.value = 'rows'
    old.dispatchEvent(new Event('input', { bubbles: true }))
    expect(visible()).toEqual(['Tighten the rows'])
  })
})
