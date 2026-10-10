import { describe, it, expect, beforeEach, vi } from 'vitest'

// A filter's rows of chips - stages, each field of tags - are told apart from whatever follows them,
// another row or the boxes, by a rule. The boxes are one group, and the count is not one.

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
        status: { String: 'percent' },
        hide: { String: 'blocked' },
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


const groups = () => Array.from(document.querySelectorAll('.overseer-filter > .filter-tags, .overseer-filter > .filter-status'))

describe('the rows of chips in a filter', () => {
  it('each have a rule after them when the boxes follow', () => {
    setupDOM()
    const app = new OverseerApp()
    app.currentDocument = docOf()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)

    expect(groups().map((g) => g.classList.contains('filter-sep'))).toEqual([true, true, false])
    expect(groups()[2].classList.contains('filter-status')).toBe(true)
    expect(document.querySelector('.filter-hide')).not.toBeNull()
  })
})
