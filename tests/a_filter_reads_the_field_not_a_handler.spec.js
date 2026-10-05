import { describe, it, expect, beforeEach, vi } from 'vitest'

// A filter reads an entry's fields, not lines that merely carry their names.
//
// A project item's stage sits before its tags and its note, and its on change copies the item
// into History with lines named after both - `- labels = $(../labels)`. The filter searched the
// entry depth-first by name, so it found those lines first, read a formula where the tags should
// be, and narrowing by a tag or by text in a note found nothing at all.

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

const label = (tag) => Object.assign(base(`Label__${tag}`, 'div'), {
  children: [valued('tag', 'string', 'String', tag), valued('name', 'string', 'String', tag)],
})

// The stage, with the handler the project tracker gives it, ahead of the fields it copies.
const stage = () => Object.assign(valued('stage', 'string', 'String', 'filed'), {
  children: [Object.assign(base('change', 'on'), {
    children: [Object.assign(base('append', 'append'), {
      parameters: { list: { String: '/project/History' } },
      children: [valued('labels', 'string', 'String', '$(../labels)'),
                 valued('commentary', 'string', 'String', '$(../commentary)')],
    })],
  })],
})

const item = (n, title, labels, commentary) => Object.assign(base(`Item__${n}`, 'div'), {
  children: [
    valued('title', 'string', 'String', title),
    stage(),
    valued('labels', 'tags', 'String', labels),
    valued('commentary', 'string', 'String', commentary),
  ],
})

const docOf = () => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('Labels', 'list'), {
      parameters: { key: { String: 'tag' } },
      children: [label('ui'), label('perf')],
    }),
    Object.assign(base('filter', 'filter'), {
      parameters: {
        target: { String: '/project/Items' },
        text: { String: 'title, commentary' },
        tags: { String: 'labels' },
        vocabulary: { String: '/project/Labels' },
      },
    }),
    Object.assign(base('Items', 'list'), {
      children: [
        item(1, 'Tighten the rows', 'ui', 'found in the drawer'),
        item(2, 'Open the documents fast', 'perf', ''),
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

describe('a filter over entries whose handlers name their fields', () => {
  beforeEach(() => {
    setupDOM()
    const app = new OverseerApp()
    app.currentDocument = docOf()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('narrows by a tag the entry holds', () => {
    Array.from(document.querySelectorAll('.filter-tag'))
      .find((chip) => chip.dataset.tag === 'ui')
      .dispatchEvent(new Event('click', { bubbles: true }))
    expect(visible()).toEqual(['Tighten the rows'])
  })

  it('matches text found only in the note', () => {
    const box = document.querySelector('.filter-text')
    box.value = 'drawer'
    box.dispatchEvent(new Event('input', { bubbles: true }))
    expect(visible()).toEqual(['Tighten the rows'])
  })

  it('does not match the formula a handler copies', () => {
    const box = document.querySelector('.filter-text')
    box.value = '../commentary'
    box.dispatchEvent(new Event('input', { bubbles: true }))
    expect(visible()).toEqual([])
  })
})
