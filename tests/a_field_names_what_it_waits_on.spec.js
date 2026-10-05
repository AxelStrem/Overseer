import { describe, it, expect, beforeEach, vi } from 'vitest'

// A task's `after` names the tasks it waits on, by handle. It is a tags field whose vocabulary is
// two lists: the open tasks, which the picker offers, and the finished ones, which only name and
// colour what is already held. An entry is known by its list's key, so a list of tasks keyed by
// handle serves as well as a tag list keyed by tag, and its title is what its chip says on hover.

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

/** A worked-out field, as the resolver hands it over. */
const computed = (name, type, value) => Object.assign(base(name, type), {
  parameters: { value: { Formula: '...' }, _computed_value: { String: value }, hidden: { Boolean: true } },
})

const open = (n, handle, title, colour, after, waiting) => Object.assign(base(`Item__${n}`, 'div'), {
  children: [
    valued('handle', 'string', 'String', handle),
    valued('title', 'string', 'String', title),
    Object.assign(valued('after', 'tags', 'String', after), {
      parameters: { value: { String: after }, vocabulary: { String: '/project/Items, /project/History' } },
    }),
    Object.assign(base('waiting', 'int'), {
      parameters: { value: { Formula: '...' }, _computed_value: { Integer: waiting }, hidden: { Boolean: true } },
    }),
    computed('colour', 'string', colour),
  ],
})

// A finished record holds its fields inside a layout div that names nothing.
const finished = (n, handle, title) => Object.assign(base(`Finished__${n}`, 'div'), {
  children: [
    Object.assign(base('', 'div'), {
      is_hierarchy_transparent: true,
      children: [valued('handle', 'string', 'String', handle), valued('title', 'string', 'String', title)],
    }),
    computed('colour', 'string', '#15803d'),
  ],
})

const docOf = () => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('filter', 'filter'), {
      parameters: { target: { String: '/project/Items' }, text: { String: 'title' }, hide: { String: 'waiting' } },
    }),
    Object.assign(base('Items', 'list'), {
      parameters: { key: { String: 'handle' } },
      children: [
        open(1, 'rows', 'Tighten the rows', '#1d4ed8', 'escape', 0),
        open(2, 'favicon', 'Pick a favicon', '#475569', 'rows, nosuch', 1),
        open(3, 'docs', 'Write the guide', '#6b7280', '', 0),
      ],
    }),
    Object.assign(base('History', 'list'), {
      parameters: { key: { String: 'handle' } },
      children: [finished(1, 'escape', 'Escape quotes when saving')],
    }),
  ],
})])

const afterOf = (handle) => Array.from(document.querySelectorAll('.tags-field')).find((field) => {
  const row = field.closest('[data-path]') && JSON.parse(field.closest('[data-path]').dataset.path)
  if (!row || row[1] !== 'Items') return false
  const entry = document.querySelector(`[data-path='${JSON.stringify(row.slice(0, 3))}']`)
  return entry && entry.textContent.includes(handle) && entry.querySelector(`[data-path='${JSON.stringify([...row.slice(0, 3), 'handle'])}']`).textContent.trim() === handle
})
const chipsOf = (handle) => Array.from(afterOf(handle).querySelectorAll('.tag-chip:not(.tag-option)'))

const visible = () => Array.from(document.querySelectorAll('[data-path]'))
  .filter((element) => {
    let path
    try { path = JSON.parse(element.dataset.path) } catch { return false }
    return path.length === 3 && path[1] === 'Items' && !element.classList.contains('filtered-out')
  })
  .map((element) => document.querySelector(`[data-path='${JSON.stringify([...JSON.parse(element.dataset.path), 'handle'])}']`).textContent.trim())

describe('a field naming the tasks it waits on', () => {
  let app

  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
    app.currentDocument = docOf()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('draws each handle in the colour of that task, finished ones too', () => {
    expect(chipsOf('favicon').map(c => c.dataset.tag)).toEqual(['rows', 'nosuch'])
    expect(chipsOf('favicon')[0].style.backgroundColor).toBe('rgb(29, 78, 216)')
    expect(chipsOf('rows')[0].style.backgroundColor).toBe('rgb(21, 128, 61)')
  })

  it('says the title on hover', () => {
    expect(chipsOf('favicon')[0].title).toBe('Tighten the rows')
    expect(chipsOf('rows')[0].title).toBe('Escape quotes when saving')
  })

  it('marks a handle that names nothing', () => {
    expect(chipsOf('favicon')[1].classList.contains('tag-unknown')).toBe(true)
  })

  it('offers the open tasks only - not ones already named, and not itself', () => {
    afterOf('docs').querySelector('.tag-add').dispatchEvent(new Event('click', { bubbles: true }))
    const offered = Array.from(document.querySelectorAll('.tag-picker .tag-option')).map(o => o.dataset.tag)
    expect(offered).toEqual(['rows', 'favicon'])

    document.body.click()
    afterOf('favicon').querySelector('.tag-add').dispatchEvent(new Event('click', { bubbles: true }))
    const more = Array.from(document.querySelectorAll('.tag-picker .tag-option')).map(o => o.dataset.tag)
    expect(more).toEqual(['docs'])
  })

  it('lets the filter leave out what is waiting', () => {
    expect(visible()).toEqual(['rows', 'favicon', 'docs'])
    const box = document.querySelector('.filter-hide input')
    expect(document.querySelector('.filter-hide').textContent).toBe('hide waiting')
    box.checked = true
    box.dispatchEvent(new Event('change', { bubbles: true }))
    expect(visible()).toEqual(['rows', 'docs'])
    expect(document.querySelector('.filter-count').textContent).toBe('2 of 3')
  })
})
