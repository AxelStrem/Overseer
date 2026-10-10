import { describe, it, expect, beforeEach, vi } from 'vitest'

// A filter that names a list among its text fields searches what the list's entries hold.
//
// A project item's conversation is a list of comments inside a folded div, each with an author
// and a body. A list has no value of its own, so `find` naming it read nothing and an item could
// no longer be found by what was said about it - which the one-line note it replaced allowed.

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

const comment = (n, author, body) => Object.assign(base(`Comment__${n}`, 'div'), {
  children: [valued('author', 'string', 'String', author), valued('body', 'text', 'String', body)],
})

// The conversation as the project template draws it: the comments, and under them the form
// that adds one, whose box may hold a draft nobody has said yet.
const talk = (comments, draft) => Object.assign(base('Talk', 'div'), {
  parameters: { foldable: { Boolean: true }, folded: { Boolean: true } },
  children: [
    Object.assign(base('Comments', 'list'), { children: comments }),
    Object.assign(base('NewComment', 'div'), {
      children: [valued('body', 'textbox', 'String', draft), base('add', 'button')],
    }),
  ],
})

const item = (n, title, comments, draft = '') => Object.assign(base(`Item__${n}`, 'div'), {
  children: [valued('title', 'string', 'String', title), talk(comments, draft)],
})

const docOf = () => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('filter', 'filter'), {
      parameters: {
        target: { String: '/project/Items' },
        text: { String: 'title, Comments' },
      },
    }),
    Object.assign(base('Items', 'list'), {
      children: [
        item(1, 'Tighten the rows', [
          comment(1, 'owner', 'the rows are too tall on a phone'),
          comment(2, 'claude-opus (high)', 'Built: the padding comes from the **drawer** styles'),
        ]),
        item(2, 'Open the documents fast', [], 'a draft about the drawer'),
        item(3, 'Nothing said yet', []),
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

const find = (text) => {
  const box = document.querySelector('.filter-text')
  box.value = text
  box.dispatchEvent(new Event('input', { bubbles: true }))
}

describe('a filter naming a list of comments', () => {
  beforeEach(() => {
    setupDOM()
    const app = new OverseerApp()
    app.currentDocument = docOf()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('matches text found only in a later comment', () => {
    find('drawer')
    expect(visible()).toEqual(['Tighten the rows'])
  })

  it('matches who said it', () => {
    find('opus')
    expect(visible()).toEqual(['Tighten the rows'])
  })

  it('does not match a draft not yet added', () => {
    find('a draft about')
    expect(visible()).toEqual([])
  })

  it('still matches the title of an item nobody has commented on', () => {
    find('nothing said')
    expect(visible()).toEqual(['Nothing said yet'])
  })
})
