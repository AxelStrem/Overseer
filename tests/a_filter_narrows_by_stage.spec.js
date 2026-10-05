import { describe, it, expect, beforeEach, vi } from 'vitest'

// A filter offers the values of an `enum` field - a project item's stage - as chips to narrow by.
// An entry holds one stage, so picking two widens rather than narrows, unlike tags. The chips come
// from the field's own vocabulary, read from the entries, so the filter is not told it twice.

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

const item = (n, title, stage, labels) => Object.assign(base(`Item__${n}`, 'div'), {
  children: [
    valued('title', 'string', 'String', title),
    Object.assign(valued('stage', 'enum', 'String', stage), {
      parameters: { value: { String: stage }, vocabulary: { String: '/project/Stages' } },
    }),
    valued('labels', 'tags', 'String', labels),
  ],
})

const docOf = () => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('Stages', 'list'), {
      parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
      children: [entryOf('Stage', 'filed', '#6b7280'), entryOf('Stage', 'asked', '#b45309'),
                 entryOf('Stage', 'ready', '#1d4ed8'), entryOf('Stage', 'later', '#475569')],
    }),
    Object.assign(base('Labels', 'list'), {
      parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
      children: [entryOf('Label', 'bug', '#d73a4a'), entryOf('Label', 'ui', '#1d76db')],
    }),
    Object.assign(base('filter', 'filter'), {
      parameters: {
        target: { String: '/project/Items' },
        text: { String: 'title' },
        enum: { String: 'stage' },
        tags: { String: 'labels' },
        vocabulary: { String: '/project/Labels' },
      },
    }),
    Object.assign(base('Items', 'list'), {
      children: [
        item(1, 'Parser drops a brace', 'ready', 'bug'),
        item(2, 'Write the guide', 'asked', ''),
        item(3, 'Tighten the rows', 'ready', 'ui'),
        item(4, 'Themes', 'later', 'ui'),
        item(5, 'Something new', 'filed', 'bug'),
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

const stageChips = () => Array.from(document.querySelectorAll('.filter-enum .filter-tag'))
const press = (value) => stageChips().find((chip) => chip.dataset.tag === value)
  .dispatchEvent(new Event('click', { bubbles: true }))
const pressTag = (tag) => Array.from(document.querySelectorAll('.filter-tags:not(.filter-enum) .filter-tag'))
  .find((chip) => chip.dataset.tag === tag).dispatchEvent(new Event('click', { bubbles: true }))

describe('a filter with stages', () => {
  let app

  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
    app.currentDocument = docOf()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('offers the stages the field lists, in its order, apart from the tags', () => {
    expect(stageChips().map((c) => c.dataset.tag)).toEqual(['filed', 'asked', 'ready', 'later'])
    expect(stageChips()[2].style.backgroundColor).toBe('rgb(29, 78, 216)')
  })

  it('narrows to a stage', () => {
    press('ready')
    expect(visible()).toEqual(['Parser drops a brace', 'Tighten the rows'])
  })

  it('two stages means either, unlike two tags', () => {
    press('ready')
    press('asked')
    expect(visible()).toEqual(['Parser drops a brace', 'Write the guide', 'Tighten the rows'])
  })

  it('combines with the tags', () => {
    press('ready')
    pressTag('ui')
    expect(visible()).toEqual(['Tighten the rows'])
  })

  it('counts what is showing', () => {
    press('later')
    expect(document.querySelector('.filter-count').textContent).toBe('1 of 5')
  })

  it('lets a stage go again', () => {
    press('later')
    press('later')
    expect(visible().length).toBe(5)
  })
})
